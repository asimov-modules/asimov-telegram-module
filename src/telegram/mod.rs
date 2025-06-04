// This is free and unencumbered software released into the public domain.

use miette::{IntoDiagnostic, Result, WrapErr, bail, miette};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString, c_char, c_int, c_void},
    format,
    path::PathBuf,
    ptr::NonNull,
    string::{String, ToString},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    vec,
    vec::Vec,
};
use tokio::sync::{RwLock, oneshot};

mod messages;
use messages::*;

unsafe extern "C" {
    fn td_json_client_create() -> *mut c_void;
    fn td_json_client_send(client: *mut c_void, request: *const c_char);
    fn td_json_client_receive(client: *mut c_void, timeout: f64) -> *const c_char;
    fn td_json_client_destroy(client: *mut c_void);
    fn td_set_log_verbosity_level(new_verbosity_level: c_int);
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Init,
    AwaitingAuthorization,
    AwaitingPhoneNumber,
    AwaitingCode,
    Authorized {
        chats: BTreeMap<i64, Value>,
        supergroups: BTreeMap<i64, Value>,
        users: BTreeMap<i64, Value>,
    },
}

#[derive(Clone)]
pub struct Config {
    pub api_id: String,
    pub api_hash: String,
    pub database_directory: PathBuf,
}

struct TdHandle(NonNull<c_void>);

// I *think* this is ok? If not will just have to start a second worker thread
// that does all the `td_json_client_send`.
unsafe impl Send for TdHandle {}
unsafe impl Sync for TdHandle {}

impl Drop for TdHandle {
    fn drop(&mut self) {
        unsafe { td_json_client_destroy(self.0.as_ptr()) }
    }
}

pub struct Client {
    config: Config,
    state: Arc<RwLock<State>>,
    handle: Arc<TdHandle>,
    requests: flume::Sender<(u64, oneshot::Sender<Value>)>,
    id_counter: AtomicU64,
}

impl Client {
    pub fn new(config: Config) -> Result<Self> {
        unsafe { td_set_log_verbosity_level(1) };
        let handle = unsafe { td_json_client_create() };
        let handle = NonNull::new(handle).ok_or_else(|| miette!("Failed to create client"))?;
        let handle = Arc::new(TdHandle(handle));

        let state = Arc::new(RwLock::new(State::default()));

        let (req_tx, req_rx) = flume::bounded(0);

        let _receiver_handle = tokio::task::spawn_blocking({
            let handle = handle.clone();
            let state = state.clone();
            move || {
                let mut pending_reqs: BTreeMap<u64, oneshot::Sender<Value>> = BTreeMap::new();
                loop {
                    match req_rx.try_recv() {
                        Err(flume::TryRecvError::Empty) => (),
                        Err(flume::TryRecvError::Disconnected) => break,
                        Ok((id, resp_tx)) => {
                            pending_reqs.insert(id, resp_tx);
                        }
                    }

                    let response_ptr = unsafe { td_json_client_receive(handle.0.as_ptr(), 0.2) };
                    if response_ptr.is_null() {
                        continue;
                    }
                    let c_str = unsafe { CStr::from_ptr(response_ptr) };
                    let resp = c_str.to_string_lossy().into_owned();
                    tracing::trace!(msg=%resp, "Received message");

                    let resp = serde_json::from_str::<Value>(&resp).unwrap();

                    if let Some(id) = resp["@extra"].as_str() {
                        id.parse()
                            .ok()
                            .and_then(|id| pending_reqs.remove(&id))
                            .and_then(|tx| tx.send(resp.clone()).ok());
                    }

                    match serde_json::from_value(resp.clone()) {
                        Ok(TdLibResponse::UpdateAuthorizationState {
                            authorization_state,
                        }) => match authorization_state.typ {
                            AuthState::AuthorizationStateWaitTdlibParameters => {
                                *state.blocking_write() = State::AwaitingAuthorization
                            }
                            AuthState::AuthorizationStateWaitPhoneNumber => {
                                *state.blocking_write() = State::AwaitingPhoneNumber
                            }
                            AuthState::AuthorizationStateWaitCode => {
                                *state.blocking_write() = State::AwaitingCode
                            }
                            AuthState::AuthorizationStateReady => {
                                *state.blocking_write() = State::Authorized {
                                    chats: Default::default(),
                                    supergroups: Default::default(),
                                    users: Default::default(),
                                };
                            }
                        },
                        Ok(TdLibResponse::UpdateNewChat { chat }) => {
                            let State::Authorized { ref mut chats, .. } = *state.blocking_write()
                            else {
                                continue;
                            };
                            chats.insert(chat.id, resp);
                        }
                        Ok(TdLibResponse::UpdateSuperGroup { supergroup }) => {
                            let State::Authorized {
                                ref mut supergroups,
                                ..
                            } = *state.blocking_write()
                            else {
                                continue;
                            };
                            supergroups.insert(supergroup.id, resp);
                        }
                        Ok(TdLibResponse::UpdateUser { user }) => {
                            let State::Authorized { ref mut users, .. } = *state.blocking_write()
                            else {
                                continue;
                            };
                            users.insert(user.id, resp);
                        }
                        Err(_) => {
                            // ignore for now
                        }
                    }
                }
            }
        });
        Ok(Client {
            config,
            state,
            handle,
            requests: req_tx,
            id_counter: AtomicU64::new(1),
        })
    }

    pub async fn init(self) -> Result<Self> {
        assert_eq!(*self.state.read().await, State::Init);

        let req = json!({
            "@type": "setTdlibParameters",
            "database_directory": self.config.database_directory,
            "api_id": self.config.api_id,
            "api_hash": self.config.api_hash,

            "use_test_dc": false,
            "use_file_database": false,
            "use_chat_info_database": false,
            "use_message_database": true,
            "use_secret_chats": true,
            "system_language_code": "en",
            "device_model": "Desktop",
            "system_version": "Unknown",
            "application_version": "1.0",
            "enable_storage_optimizer": true,
            "ignore_file_names": false
        });
        let resp = self.request(req).await?;

        assert_ok_response(resp).context("TdLib client initialization failed")?;

        for _ in 0..10 {
            match *self.state.read().await {
                State::AwaitingPhoneNumber | State::AwaitingCode | State::Authorized { .. } => {
                    break;
                }
                _ => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
            }
        }

        Ok(self)
    }

    pub async fn is_authorised(&self) -> bool {
        matches!(*self.state.read().await, State::Authorized { .. })
    }

    pub async fn is_need_code(&self) -> bool {
        matches!(*self.state.read().await, State::AwaitingCode)
    }

    pub async fn send_auth_request(&self, phone_number: &str) -> Result<()> {
        assert_eq!(*self.state.read().await, State::AwaitingPhoneNumber);

        let req = json!({ "@type": "setAuthenticationPhoneNumber", "phone_number": phone_number });
        let resp = self.request(req).await?;
        assert_ok_response(resp).context("Failed to request authentication code")?;

        Ok(())
    }

    pub async fn send_auth_code(&self, code: &str) -> Result<()> {
        assert_eq!(*self.state.read().await, State::AwaitingCode);

        let req = json!({ "@type": "checkAuthenticationCode", "code": code });
        let resp = self.request(req).await?;
        assert_ok_response(resp).context("Failed to confirm authentication code")?;

        Ok(())
    }

    pub async fn get_chat_ids(&self) -> Result<BTreeSet<i64>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        let State::Authorized { ref chats, .. } = *self.state.read().await else {
            return Ok(Default::default());
        };

        Ok(chats.iter().map(|(id, _)| *id).collect())
    }

    pub async fn get_chats(&self) -> Result<BTreeMap<i64, Value>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        self.load_chats().await.context("Failed to load chats")?;

        let State::Authorized { ref chats, .. } = *self.state.read().await else {
            return Ok(Default::default());
        };

        Ok(chats.clone())
    }

    pub async fn get_chat_members(&self) -> Result<BTreeMap<i64, Vec<Value>>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        let mut members: BTreeMap<i64, Vec<Value>> = BTreeMap::new();

        let chats = self.get_chats().await.context("Failed to get chat IDs")?;

        let basicgroups: Vec<(i64, &Value)> = chats
            .iter()
            .filter_map(|(id, val)| {
                let is_basicgroup = val["chat"]["type"]["@type"].as_str()? == "chatTypeBasicGroup";
                if is_basicgroup {
                    Some((*id, val))
                } else {
                    None
                }
            })
            .collect();

        for (id, _g) in basicgroups {
            let group_members = self.get_basicgroup_members(id).await?;
            members.entry(id).or_default().extend(group_members);
        }

        let supergroups: Vec<(i64, &Value)> = chats
            .iter()
            .filter_map(|(_, val)| {
                let is_supergroup = val["chat"]["type"]["@type"].as_str()? == "chatTypeSupergroup";
                let is_channel = val["chat"]["type"]["is_channel"].as_bool()?;
                let sg_id = val["type"]["supergroup_id"].as_i64()?;

                if is_supergroup && !is_channel {
                    Some((sg_id, val))
                } else {
                    None
                }
            })
            .collect();

        for (id, _sg) in supergroups {
            let group_members = self.get_supergroup_members(id, None).await?;
            members.entry(id).or_default().extend(group_members);
        }

        Ok(members)
    }

    pub async fn get_basicgroup_members(&self, basicgroup_id: i64) -> Result<Vec<Value>> {
        let req = json!({ "@type": "getBasicGroupFullInfo", "basic_group_id": basicgroup_id });
        let resp = self.request(req).await?;

        let group_members = resp["basicGroupFullInfo"]["members"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        Ok(group_members)
    }

    pub async fn get_supergroup_members(
        &self,
        supergroup_id: i64,
        max_amount: Option<usize>,
    ) -> Result<Vec<Value>> {
        let mut members = Vec::new();
        loop {
            let req = json!({ "@type": "getSupergroupMembers", "supergroup_id": supergroup_id, "offset": members.len(), "limit": 200 });
            let resp = self.request(req).await?;

            let Some(group_members) = resp["members"].as_array() else {
                break;
            };

            if group_members.is_empty() {
                break;
            }

            if let Some(max) = max_amount {
                let remaining = max.saturating_sub(members.len());
                if remaining == 0 {
                    break;
                }
                let take_amount = remaining.min(group_members.len());
                members.extend_from_slice(&group_members[..take_amount]);
                if members.len() >= max {
                    break;
                }
            } else {
                members.extend_from_slice(group_members);
            }
        }

        Ok(members)
    }

    pub async fn get_supergroups(&self) -> Result<BTreeMap<i64, Value>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        self.load_chats().await.context("Failed to load chats")?;

        let State::Authorized {
            ref supergroups, ..
        } = *self.state.read().await
        else {
            return Ok(Default::default());
        };

        Ok(supergroups.clone())
    }

    pub async fn get_users(&self) -> Result<BTreeMap<i64, Value>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        let State::Authorized { ref users, .. } = *self.state.read().await else {
            return Ok(Default::default());
        };

        Ok(users.clone())
    }

    async fn load_chats(&self) -> Result<()> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        let chat_lists = vec![
            json!({"@type": "chatListMain"}),
            json!({"@type": "chatListArchive"}),
        ];

        for list in chat_lists {
            loop {
                let req = json!({ "@type": "loadChats", "chat_list": list, "limit": "50" });

                let resp = self.request(req).await?;
                let is_ok = resp["@type"].as_str().is_some_and(|t| t == "ok");
                if is_ok {
                    continue;
                };

                // {"@type":"error","code":404,"message":"Not Found","@extra":"1"}
                let is_404 = resp["code"].as_i64().is_some_and(|c| c == 404);
                if is_404 {
                    // All chats have been loaded
                    break;
                }

                bail!("Unknown error: {resp}");
            }
        }

        Ok(())
    }

    async fn request(&self, mut msg: Value) -> Result<Value> {
        let extra_id = self.id_counter.fetch_add(1, Ordering::SeqCst);
        msg["@extra"] = extra_id.to_string().into();

        tracing::debug!(extra=extra_id, %msg, "Sending request message");

        let (resp_tx, resp_rx) = oneshot::channel();
        self.requests
            .send_async((extra_id, resp_tx))
            .await
            .map_err(|_| miette!("Failed to send"))?;

        let msg = msg.to_string();

        let c_request = CString::new(msg).unwrap();
        unsafe { td_json_client_send(self.handle.0.as_ptr(), c_request.as_ptr()) };

        let resp = resp_rx
            .await
            .into_diagnostic()
            .wrap_err_with(|| miette!("No response for request"))?;

        tracing::debug!(extra=extra_id, msg=%resp, "Got response");

        Ok(resp)
    }
}

fn assert_ok_response(response: Value) -> Result<()> {
    let Some(resp) = response.as_object() else {
        return Err(miette!("Response not a JSON object"));
    };
    match resp["@type"].as_str() {
        Some("ok") => Ok(()),
        Some("error") => {
            let msg = resp["message"].as_str().unwrap_or_default();
            Err(miette!("{msg}"))
        }
        Some(_) | None => Err(miette!("Unknown failure")),
    }
}

#[cfg(test)]
mod test {
    use super::TdLibResponse;

    #[test]
    fn test_filter() {
        let s = r#"{"@type":"ok","@extra":"1"}"#;
        let filter = crate::jq::filter();
        let res = filter.filter_json_str(s);

        panic!("{res:?}");
    }

    #[test]
    fn test_parse() {
        let s = r#"{"@type":"ok","@extra":"1"}"#;
        serde_json::from_str::<TdLibResponse>(s).unwrap();

        let s = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitTdlibParameters"}}"#;
        serde_json::from_str::<TdLibResponse>(s).unwrap();
    }
}
