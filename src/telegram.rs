// This is free and unencumbered software released into the public domain.

use miette::{IntoDiagnostic, Result, WrapErr, bail, miette};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    format,
    path::PathBuf,
    string::String,
    sync::Arc,
    vec::Vec,
};
use tdlib_rs::types::Message;
use tokio::sync::RwLock;

// Have to do this manually. If you use tdlib-rs's provided
// `tdlib_rs::functions::set_log_verbosity_level` you *will* get output on stdout because that one
// is called *after* the client is created (and hence it gets a chance to start logging...).
// So, to get absolutely no output from TdLib we link and call this:
#[link(name = "tdjson_static", kind = "static")]
unsafe extern "C" {
    fn td_set_log_verbosity_level(new_verbosity_level: std::ffi::c_int);
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Init,
    AwaitingPhoneNumber,
    AwaitingCode,
    AwaitingPassword {
        hint: String,
    },
    Authorized {
        chats: BTreeMap<i64, Value>,
    },
}

#[derive(Clone)]
pub struct Config {
    pub api_id: String,
    pub api_hash: String,
    pub database_directory: PathBuf,
    pub encryption_key: String,
}

struct TdHandle(i32);

// I *think* this is ok? If not will just have to start a second worker thread
// that does all the `td_json_client_send`.
unsafe impl Send for TdHandle {}
unsafe impl Sync for TdHandle {}

impl Drop for TdHandle {
    fn drop(&mut self) {
        tracing::trace!("Closing TdLib handle");
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
                        _ => tracing::trace!(?update),
                    };

                    use tdlib_rs::enums::AuthorizationState::*;
                    match update {
                        AuthorizationState(st) => match st.authorization_state {
                            WaitTdlibParameters => *state.blocking_write() = State::Init,
                            WaitPhoneNumber => *state.blocking_write() = State::AwaitingPhoneNumber,
                            WaitCode(_) => *state.blocking_write() = State::AwaitingCode,
                            WaitPassword(x) => {
                                *state.blocking_write() = State::AwaitingPassword {
                                    hint: x.password_hint,
                                }
                            }
                            Ready => {
                                *state.blocking_write() = State::Authorized {
                                    chats: BTreeMap::new(),
                                }
                            }
                            Closed => break,
                            WaitEmailAddress(_)
                            | WaitEmailCode(_)
                            | WaitOtherDeviceConfirmation(_)
                            | WaitRegistration(_)
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
            self.config.encryption_key.clone(),
            true,
            true,
            true,
            false,
            self.config
                .api_id
                .parse()
                .map_err(|e| miette!("Invalid API_ID (`{}`): {e}", self.config.api_id))?,
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

    pub async fn is_need_password(&self, hint: &mut String) -> bool {
        if let State::AwaitingPassword { hint: ref hint2 } = *self.state.read().await {
            *hint = hint2.clone();
            return true;
        }

        false
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

    pub async fn send_auth_password(&self, password: &str) -> Result<(), tdlib_rs::types::Error> {
        assert!(matches!(
            *self.state.read().await,
            State::AwaitingPassword { .. }
        ));

        tdlib_rs::functions::check_authentication_password(password.into(), self.handle.0).await
    }

    pub async fn get_chat_ids(&self) -> Result<BTreeSet<i64>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        self.load_chats().await.context("Failed to load chats")?;

        let State::Authorized { ref chats, .. } = *self.state.read().await else {
            bail!("Unexpectedly got unauthorized");
        };

        Ok(chats.keys().cloned().collect())
    }

    pub async fn get_chats(&self) -> Result<BTreeMap<i64, Value>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        self.load_chats().await.context("Failed to load chats")?;

        let State::Authorized { ref chats, .. } = *self.state.read().await else {
            bail!("Unexpectedly got unauthorized");
        };

        Ok(chats.clone())
    }

    pub async fn get_chat_info(&self, chat_id: i64) -> Result<Value> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        self.load_chats().await.context("Failed to load chats")?;

        let State::Authorized { ref chats, .. } = *self.state.read().await else {
            bail!("Unexpectedly got unauthorized");
        };

        chats
            .get(&chat_id)
            .cloned()
            .ok_or(miette!("Unknown chat ID: {chat_id}"))
    }

    pub async fn get_chat_members(
        &self,
        chat_id: i64,
        limit: Option<usize>,
    ) -> Result<impl futures::Stream<Item = Result<Value>>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        self.load_chats().await.context("Failed to load chats")?;

        let State::Authorized { ref chats, .. } = *self.state.read().await else {
            bail!("Unexpectedly got unauthorized");
        };

        let chat = chats
            .get(&chat_id)
            .ok_or(miette!("Unknown chat ID: {chat_id}"))?;

        use tdlib_rs::{
            enums::ChatType::*,
            types::Chat,
            types::{ChatTypeBasicGroup, ChatTypePrivate, ChatTypeSecret, ChatTypeSupergroup},
        };

        let chat: Chat = serde_json::from_value(chat.clone()).unwrap();

        let stream = async_stream::try_stream! {
            match chat.r#type {
                BasicGroup(ChatTypeBasicGroup { basic_group_id }) => {
                    let members = self.get_basicgroup_members(basic_group_id).await?;
                    let members = if let Some(limit) = limit
                        && let Some((left, _right)) = members.split_at_checked(limit)
                    {
                        left.into()
                    } else {
                        members
                    };

                    for member in members {
                        yield member;
                    }
                }
                Supergroup(ChatTypeSupergroup { supergroup_id, .. }) => {
                    let members = self.get_supergroup_members(supergroup_id, limit).await?;

                    for await member in members {
                            yield member?;
                    }
                }
                Private(ChatTypePrivate { user_id }) | Secret(ChatTypeSecret { user_id, .. }) => {
                    let member = tdlib_rs::functions::get_chat_member(
                        chat_id,
                        tdlib_rs::enums::MessageSender::User(tdlib_rs::types::MessageSenderUser { user_id }),
                        self.handle.0,
                    )
                    .await
                    .map_err(|e| miette!("Failed to fetch chat member: {}", e.message))?;

                    yield serde_json::to_value(member).into_diagnostic()?;
                }
            }
        };

        Ok(stream)
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
        limit: Option<usize>,
    ) -> Result<impl futures::Stream<Item = Result<Value>>> {
        let stream = async_stream::try_stream! {
            let mut count = 0usize;
            loop {
                let limit = if let Some(max) = limit {
                    max.saturating_sub(count).min(200)
                } else {
                    200
                };
                if limit == 0 {
                    break;
                }

                tracing::debug!(count = count, limit, "fetching members...");

                let res = tokio::time::timeout(
                    std::time::Duration::from_secs(60),
                    tdlib_rs::functions::get_supergroup_members(
                        supergroup_id,
                        None,
                        count as i32,
                        limit as i32,
                        self.handle.0,
                    ),
                )
                .await
                .map_err(|_| miette!("Request timed out"))?;

                match res {
                    Ok(tdlib_rs::enums::ChatMembers::ChatMembers(tdlib_rs::types::ChatMembers { members, .. })) => {
                        if members.is_empty() {
                            break;
                        }
                        for member in members {
                            let member = serde_json::to_value(member).into_diagnostic()?;
                            count += 1;
                            yield member;
                        }
                    }
                    // {"@type":"error","code":400,"message":"Member list is inaccessible","@extra":"1"}
                    Err(err) if err.code == 400 => break,
                    Err(err) => Err(miette!("Failed to get chat members: {}", err.message))?,
                };
            }
        };

        Ok(stream)
    }

    pub async fn get_user(&self, user_id: i64) -> Result<Value> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        tdlib_rs::functions::get_user(user_id, self.handle.0)
            .await
            .map_err(|e| miette!("Failed to get user: {}", e.message))
            .and_then(|user| serde_json::to_value(user).into_diagnostic())
    }

    async fn load_chats(&self) -> Result<()> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        let chat_lists = std::vec![
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

    pub async fn get_chat_history(
        &self,
        chat_id: i64,
        from_msg_id: Option<i64>,
        limit: Option<usize>,
    ) -> Result<impl futures::Stream<Item = Result<Message>>> {
        assert!(matches!(*self.state.read().await, State::Authorized { .. }));

        self.load_chats().await.context("Failed to load chats")?;

        if let State::Authorized { ref chats, .. } = *self.state.read().await {
            if !chats.contains_key(&chat_id) {
                bail!("Chat ID {} not found", chat_id);
            }
        } else {
            bail!("Unexpectedly got unauthorized");
        };

        let stream = async_stream::try_stream! {
            let mut from_msg_id = from_msg_id;
            let mut count = 0usize;

            loop {
                let limit = if let Some(limit) = limit {
                    limit.saturating_sub(count).min(100)
                } else {
                    100
                };
                if limit == 0 {
                    break;
                }

                tracing::debug!(count = count, limit, "fetching messages...");

                let tdlib_rs::enums::Messages::Messages(batch) = tokio::time::timeout(
                    std::time::Duration::from_secs(60),
                    tdlib_rs::functions::get_chat_history(
                        chat_id,
                        from_msg_id.unwrap_or(0),
                        0,
                        limit as i32,
                        false,
                        self.handle.0,
                    ),
                )
                .await
                .map_err(|_| miette!("Request timed out"))?
                .map_err(|e| miette!("Failed to get chat history: {}", e.message))?;

                let msgs: Vec<Message> = batch.messages.into_iter().flatten().collect();

                if msgs.is_empty() {
                    break;
                }

                for msg in msgs {
                    from_msg_id = match from_msg_id {
                        Some(old_id) if old_id < msg.id => old_id,
                        Some(_) => msg.id,
                        None => msg.id,
                    }.into();
                    count += 1;
                    yield msg;
                }
            }
        };

        Ok(stream)
    }
}

pub fn get_or_create_encryption_key() -> Result<String> {
    let entry = keyring::Entry::new("asimov-telegram-module", "tdlib-encryption-key")
        .map_err(|e| miette!("Failed to create keyring entry: {e}"))?;

    match entry.get_password() {
        Ok(key) => {
            tracing::debug!("Retrieved existing encryption key from keyring");
            Ok(key)
        }
        Err(keyring::Error::NoEntry) => {
            // Generate a new key
            let key = {
                use rand::RngCore;
                let mut key_bytes = [0u8; 32];
                let mut rng = rand::rngs::OsRng;
                rng.fill_bytes(&mut key_bytes);
                hex::encode(key_bytes)
            };
            entry
                .set_password(&key)
                .map_err(|e| miette!("Failed to store new encryption key in keyring: {e}"))?;
            tracing::debug!("Generated and stored new encryption key in keyring");
            Ok(key)
        }
        Err(e) => Err(miette!(
            "Failed to retrieve encryption key from keyring: {e}"
        )),
    }
}
