// This is free and unencumbered software released into the public domain.

use miette::{IntoDiagnostic, Result, WrapErr, bail, miette};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    format,
    path::PathBuf,
    string::String,
    sync::Arc,
    vec,
    vec::Vec,
};
use tokio::sync::RwLock;

// Have to do this manually. If you use tdlib-rs's provided
// `tdlib_rs::functions::set_log_verbosity_level` you *will* get output on stdout because that one
// is called *after* the client is created (and hence it gets a chance to start logging...).
// So, to get absolutely no output from TdLib we link and call this:
#[link(name = "tdjson")]
unsafe extern "C" {
    fn td_set_log_verbosity_level(new_verbosity_level: std::ffi::c_int);
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Init,
    AwaitingPhoneNumber,
    AwaitingCode,
    Authorized {
        chats: BTreeMap<i64, Value>,
        basicgroups: BTreeMap<i64, Value>,
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

struct TdHandle(i32);

// I *think* this is ok? If not will just have to start a second worker thread
// that does all the `td_json_client_send`.
unsafe impl Send for TdHandle {}
unsafe impl Sync for TdHandle {}

impl Drop for TdHandle {
    fn drop(&mut self) {
        tracing::debug!("Closing TdLib handle");
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(tdlib_rs::functions::close(self.0))
                .unwrap();
        });
    }
}

pub struct Client {
    config: Config,
    state: Arc<RwLock<State>>,
    handle: Arc<TdHandle>,
}

impl Client {
    pub fn new(config: Config) -> Result<Self> {
        unsafe { td_set_log_verbosity_level(0) };

        let handle = tdlib_rs::create_client();
        let handle = Arc::new(TdHandle(handle));

        let state = Arc::new(RwLock::new(State::default()));

        let _receiver_handle = tokio::task::spawn_blocking({
            let state = state.clone();

            move || {
                loop {
                    let Some((update, _client_id)) = tdlib_rs::receive() else {
                        continue;
                    };

                    use tdlib_rs::enums::Update::*;

                    match update {
                        Option(_) => (),
                        _ => tracing::debug!(?update),
                    };

                    use tdlib_rs::enums::AuthorizationState::*;
                    match update {
                        AuthorizationState(st) => match st.authorization_state {
                            WaitTdlibParameters => *state.blocking_write() = State::Init,
                            WaitPhoneNumber => *state.blocking_write() = State::AwaitingPhoneNumber,
                            WaitCode(_) => *state.blocking_write() = State::AwaitingCode,
                            Ready => {
                                *state.blocking_write() = State::Authorized {
                                    chats: BTreeMap::new(),
                                    basicgroups: BTreeMap::new(),
                                    supergroups: BTreeMap::new(),
                                    users: BTreeMap::new(),
                                }
                            }
                            Closed => break,
                            WaitEmailAddress(_)
                            | WaitEmailCode(_)
                            | WaitOtherDeviceConfirmation(_)
                            | WaitRegistration(_)
                            | WaitPassword(_)
                            | LoggingOut
                            | Closing => (), // ignore
                        },
                        NewChat(chat) => {
                            let State::Authorized { ref mut chats, .. } = *state.blocking_write()
                            else {
                                continue;
                            };

                            chats.insert(chat.chat.id, serde_json::to_value(chat.chat).unwrap());
                        }
                        Supergroup(supergroup) => {
                            let State::Authorized {
                                ref mut supergroups,
                                ..
                            } = *state.blocking_write()
                            else {
                                continue;
                            };

                            supergroups.insert(
                                supergroup.supergroup.id,
                                serde_json::to_value(supergroup.supergroup).unwrap(),
                            );
                        }
                        BasicGroup(basicgroup) => {
                            let State::Authorized {
                                ref mut basicgroups,
                                ..
                            } = *state.blocking_write()
                            else {
                                continue;
                            };

                            basicgroups.insert(
                                basicgroup.basic_group.id,
                                serde_json::to_value(basicgroup.basic_group).unwrap(),
                            );
                        }
                        User(user) => {
                            let State::Authorized { ref mut users, .. } = *state.blocking_write()
                            else {
                                continue;
                            };

                            users.insert(user.user.id, serde_json::to_value(user.user).unwrap());
                        }
                        _ => (), // ignore
                    }
                }
            }
        });
        Ok(Client {
            config,
            state,
            handle,
        })
    }

    pub async fn init(self) -> Result<Self> {
        assert_eq!(*self.state.read().await, State::Init);

        tdlib_rs::functions::set_tdlib_parameters(
            false,
            self.config.database_directory.to_string_lossy().into(),
            "".into(),
            "".into(), // TODO: create, save, and fetch a key securely (i.e. to keychain on macos)
            true,
            true,
            true,
            false,
            self.config.api_id.parse().unwrap(),
            self.config.api_hash.clone(),
            "en".into(),
            "Desktop".into(),
            "".into(),
            "1.0".into(),
            self.handle.0,
        )
        .await
        .map_err(|e| miette!("TdLib client initialization failed: {}", e.message))?;

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

        tdlib_rs::functions::set_authentication_phone_number(
            phone_number.into(),
            None,
            self.handle.0,
        )
        .await
        .map_err(|e| miette!("Failed to request authentication code: {}", e.message))
    }

    pub async fn send_auth_code(&self, code: &str) -> Result<()> {
        assert_eq!(*self.state.read().await, State::AwaitingCode);

        tdlib_rs::functions::check_authentication_code(code.into(), self.handle.0)
            .await
            .map_err(|e| miette!("Failed to confirm authentication code: {}", e.message))
    }

    pub async fn get_chat_ids(&self) -> Result<BTreeSet<i64>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        self.load_chats().await.context("Failed to load chats")?;

        let State::Authorized { ref chats, .. } = *self.state.read().await else {
            return Ok(Default::default());
        };

        Ok(chats.iter().map(|(id, _)| *id).collect())
    }

    pub async fn get_chats(&self) -> Result<BTreeMap<i64, Value>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        self.load_chats().await.context("Failed to load chats")?;

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let State::Authorized { ref chats, .. } = *self.state.read().await else {
            return Ok(Default::default());
        };

        Ok(chats.clone())
    }

    pub async fn get_basicgroups(&self) -> Result<BTreeMap<i64, Value>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        self.load_chats().await.context("Failed to load chats")?;

        let State::Authorized {
            ref basicgroups, ..
        } = *self.state.read().await
        else {
            return Ok(Default::default());
        };

        Ok(basicgroups.clone())
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

    pub async fn get_group_members(
        &self,
        max_per_group: Option<usize>,
    ) -> Result<BTreeMap<i64, Vec<Value>>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        self.load_chats().await.context("Failed to load chats")?;

        let (basicgroups, supergroups) = {
            let State::Authorized {
                ref basicgroups,
                ref supergroups,
                ..
            } = *self.state.read().await
            else {
                return Ok(Default::default());
            };
            (
                basicgroups.keys().cloned().collect::<Vec<_>>(),
                supergroups.keys().cloned().collect::<Vec<_>>(),
            )
        };

        let mut members: BTreeMap<i64, Vec<Value>> = BTreeMap::new();

        for id in basicgroups {
            let group_members = self.get_basicgroup_members(id).await?;
            members.entry(id).or_default().extend(group_members);
        }

        for id in supergroups {
            let tdlib_rs::enums::SupergroupFullInfo::SupergroupFullInfo(info) =
                tdlib_rs::functions::get_supergroup_full_info(id, self.handle.0)
                    .await
                    .map_err(|e| miette!(e.message))?;

            if !info.can_get_members {
                continue;
            };

            let group_members = self.get_supergroup_members(id, max_per_group).await?;
            members.entry(id).or_default().extend(group_members);
        }

        Ok(members)
    }

    pub async fn get_basicgroup_members(&self, basicgroup_id: i64) -> Result<Vec<Value>> {
        tdlib_rs::functions::get_basic_group_full_info(basicgroup_id, self.handle.0)
            .await
            .map_err(|e| miette!(e.message))
            .into_iter()
            .flat_map(|tdlib_rs::enums::BasicGroupFullInfo::BasicGroupFullInfo(info)| info.members)
            .map(|member| serde_json::to_value(member).into_diagnostic())
            .collect()
    }

    pub async fn get_supergroup_members(
        &self,
        supergroup_id: i64,
        max_amount: Option<usize>,
    ) -> Result<Vec<Value>> {
        let mut group_members = Vec::new();
        loop {
            let limit = if let Some(max) = max_amount {
                max.saturating_sub(group_members.len()).min(200)
            } else {
                200
            };
            if limit == 0 {
                break;
            }

            let res = tdlib_rs::functions::get_supergroup_members(
                supergroup_id,
                None,
                group_members.len() as i32,
                limit as i32,
                self.handle.0,
            )
            .await;

            match res {
                Ok(tdlib_rs::enums::ChatMembers::ChatMembers(members)) => {
                    group_members.extend(
                        members
                            .members
                            .iter()
                            .filter_map(|m| serde_json::to_value(m).ok()),
                    );
                    if group_members.len() >= members.total_count as usize {
                        break;
                    }
                }
                // {"@type":"error","code":400,"message":"Member list is inaccessible","@extra":"1"}
                Err(err) if err.code == 400 => break,
                Err(err) => bail!(err.message),
            }
        }

        Ok(group_members)
    }

    pub async fn get_users(&self) -> Result<BTreeMap<i64, Value>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        self.load_chats().await.context("Failed to load chats")?;

        let State::Authorized { ref users, .. } = *self.state.read().await else {
            return Ok(Default::default());
        };

        Ok(users.clone())
    }

    async fn load_chats(&self) -> Result<()> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        let chat_lists = vec![
            tdlib_rs::enums::ChatList::Main,
            tdlib_rs::enums::ChatList::Archive,
        ];

        for list in chat_lists {
            loop {
                match tdlib_rs::functions::load_chats(Some(list.clone()), 100, self.handle.0).await
                {
                    Ok(_) => (),
                    Err(err) if err.code == 404 => break,
                    Err(err) => bail!(err.message),
                }
            }
        }

        Ok(())
    }
}
