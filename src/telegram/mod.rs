// This is free and unencumbered software released into the public domain.

use miette::{IntoDiagnostic, Result, WrapErr, miette};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString, c_char, c_int, c_void},
    format,
    path::PathBuf,
    ptr::NonNull,
    string::{String, ToString},
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
    vec::Vec,
};
use tokio::sync::{
    RwLock,
    mpsc::{self, error::TryRecvError},
    oneshot,
};

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
        known_chats: BTreeSet<i64>,
        chat_data: BTreeMap<i64, ChatData>,
        supergroup_data: BTreeMap<i64, SupergroupData>,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatData {
    pub title: Option<String>,
    pub supergroup: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SupergroupData {
    pub usernames: BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub api_id: String,
    pub api_hash: String,
    pub database_directory: PathBuf,
}

struct TdHandle(NonNull<c_void>);

unsafe impl Send for TdHandle {}
unsafe impl Sync for TdHandle {}

impl Drop for TdHandle {
    fn drop(&mut self) {
        unsafe { td_json_client_destroy(self.0.as_ptr()) }
    }
}

pub struct Client {
    state: Arc<RwLock<State>>,
    handle: Arc<TdHandle>,
    requests: mpsc::Sender<(u64, oneshot::Sender<Value>)>,
    id_counter: AtomicU64,
}

impl Client {
    pub fn new() -> Result<Self> {
        unsafe { td_set_log_verbosity_level(1) };
        let handle = unsafe { td_json_client_create() };
        let handle = NonNull::new(handle).ok_or_else(|| miette!("Failed to create client"))?;
        let handle = Arc::new(TdHandle(handle));

        let state = Arc::new(RwLock::new(State::default()));

        let (req_tx, req_rx) = mpsc::channel(1);

        let _receiver_handle = tokio::task::spawn_blocking({
            let handle = handle.clone();
            let state = state.clone();
            move || {
                let mut req_rx = req_rx;
                let mut pending_reqs: BTreeMap<u64, oneshot::Sender<Value>> = BTreeMap::new();
                loop {
                    match req_rx.try_recv() {
                        Err(TryRecvError::Empty) => (),
                        Err(TryRecvError::Disconnected) => break,
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
                    tracing::debug!("Receiver got: {resp}");

                    let resp = serde_json::from_str::<Value>(&resp).unwrap();

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
                                    known_chats: BTreeSet::default(),
                                    chat_data: BTreeMap::default(),
                                    supergroup_data: BTreeMap::default(),
                                }
                            }
                        },
                        Ok(TdLibResponse::Ok { extra }) => {
                            extra
                                .parse()
                                .ok()
                                .and_then(|id| pending_reqs.remove(&id))
                                .and_then(|tx| tx.send(resp).ok());
                        }
                        Ok(TdLibResponse::Chats {
                            extra,
                            total_count: _,
                            chat_ids,
                        }) => {
                            if let State::Authorized {
                                ref mut known_chats,
                                ..
                            } = *state.blocking_write()
                            {
                                known_chats.extend(chat_ids);
                            }
                            extra
                                .parse()
                                .ok()
                                .and_then(|id| pending_reqs.remove(&id))
                                .and_then(|tx| tx.send(resp).ok());
                        }
                        Ok(TdLibResponse::UpdateNewChat { chat }) => {
                            let State::Authorized {
                                ref mut chat_data, ..
                            } = *state.blocking_write()
                            else {
                                continue;
                            };

                            let entry = chat_data.entry(chat.id).or_default();
                            entry.title = Some(chat.title);
                            if let Some(typ) = chat.other.get("type").and_then(Value::as_object) {
                                if typ
                                    .get("@type")
                                    .and_then(Value::as_str)
                                    .is_some_and(|typ| typ == "chatTypeSupergroup")
                                {
                                    if let Some(id) =
                                        typ.get("supergroup_id").and_then(Value::as_i64)
                                    {
                                        entry.supergroup = Some(id)
                                    }
                                }
                            }
                        }
                        Ok(TdLibResponse::UpdateSuperGroup { supergroup }) => {
                            let State::Authorized {
                                ref mut supergroup_data,
                                ..
                            } = *state.blocking_write()
                            else {
                                continue;
                            };

                            let entry = supergroup_data.entry(supergroup.id).or_default();

                            let usernames: Vec<String> = supergroup
                                .other
                                .get("usernames")
                                .and_then(Value::as_object)
                                .and_then(|o| o.get("active_usernames"))
                                .and_then(Value::as_array)
                                .cloned()
                                .unwrap_or_else(Vec::new)
                                .iter()
                                .filter_map(Value::as_str)
                                .map(String::from)
                                .collect();

                            entry.usernames.extend(usernames)
                        }
                        Err(_) => {
                            // ignore for now
                        }
                    }
                }
            }
        });
        Ok(Client {
            state,
            handle,
            requests: req_tx,
            id_counter: AtomicU64::new(1),
        })
    }

    pub async fn init(self, config: Config) -> Result<Self> {
        assert_eq!(*self.state.read().await, State::Init);

        let req = json!({
            "@type": "setTdlibParameters",
            "database_directory": config.database_directory,
            "api_id": config.api_id,
            "api_hash": config.api_hash,

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

    pub async fn needs_code(&self) -> bool {
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

    pub async fn get_chats(&self) -> Result<BTreeMap<i64, ChatData>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        let req = json!({ "@type": "getChats", "chat_list": null, "limit": "50" });
        let resp = self.request(req).await.context("Failed to get chats")?;

        let _chat_ids = resp
            .as_object()
            .and_then(|o| o.get("chat_ids"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(Vec::new);

        let State::Authorized { ref chat_data, .. } = *self.state.read().await else {
            return Ok(Default::default());
        };

        Ok(chat_data.clone())
    }

    pub async fn get_supergroups(&self) -> Result<BTreeMap<i64, SupergroupData>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        // TODO: Gets stuck when callen after `get_chats`?

        // let req = json!({ "@type": "getChats", "chat_list": null, "limit": "50" });
        // let resp = self
        //     .request(req)
        //     .await
        //     .context("Failed to get supergroups")?;
        //
        // let _chat_ids = resp
        //     .as_object()
        //     .and_then(|o| o.get("chat_ids"))
        //     .and_then(Value::as_array)
        //     .cloned()
        //     .unwrap_or_else(Vec::new);

        let State::Authorized {
            ref supergroup_data,
            ..
        } = *self.state.read().await
        else {
            return Ok(Default::default());
        };

        Ok(supergroup_data.clone())
    }

    async fn request(&self, mut msg: Value) -> Result<Value> {
        let extra_id = self.id_counter.fetch_add(1, Ordering::SeqCst);
        msg["@extra"] = extra_id.to_string().into();

        let (resp_tx, resp_rx) = oneshot::channel();
        self.requests
            .send((extra_id, resp_tx))
            .await
            .map_err(|_| miette!("Failed to send"))?;

        let msg = msg.to_string();

        tracing::debug!("Sending msg: id:{extra_id} msg:{msg}");

        let c_request = CString::new(msg).unwrap();
        unsafe { td_json_client_send(self.handle.0.as_ptr(), c_request.as_ptr()) };

        let resp = resp_rx
            .await
            .into_diagnostic()
            .wrap_err_with(|| miette!("No response for request"))?;

        Ok(resp)
    }
}

fn assert_ok_response(response: Value) -> Result<()> {
    let Some(resp) = response.as_object() else {
        return Err(miette!("Response not a JSON object"));
    };
    match resp.get("@type").and_then(Value::as_str) {
        Some("ok") => Ok(()),
        Some("error") => {
            let msg = resp
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Err(miette!("{msg}"))
        }
        Some(_) | None => Err(miette!("Unknown failure")),
    }
}

#[cfg(test)]
mod test {
    use super::TdLibResponse;

    #[test]
    fn test_parse() {
        let s = r#"{"@type":"ok","@extra":"1"}"#;
        serde_json::from_str::<TdLibResponse>(s).unwrap();

        let s = r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateWaitTdlibParameters"}}"#;
        serde_json::from_str::<TdLibResponse>(s).unwrap();
    }
}
