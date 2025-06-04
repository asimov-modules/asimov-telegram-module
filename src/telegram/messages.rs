// This is free and unencumbered software released into the public domain.

use serde_json::Value;
use std::{collections::BTreeMap, string::String, vec::Vec};

#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "@type", rename_all = "camelCase")]
pub enum TdLibResponse {
    UpdateAuthorizationState {
        authorization_state: UpdateAuthState,
    },
    UpdateSuperGroup {
        supergroup: UpdateSupergroup,
    },
    UpdateNewChat {
        chat: UpdateNewChat,
    },
    Messages {
        messages: Messages,
    },
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct UpdateAuthState {
    #[serde(rename = "@type")]
    pub typ: AuthState,

    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::enum_variant_names)]
pub enum AuthState {
    AuthorizationStateWaitTdlibParameters,
    AuthorizationStateWaitPhoneNumber,
    AuthorizationStateWaitCode,
    AuthorizationStateReady,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct UpdateNewChat {
    #[serde(rename = "@type")]
    pub typ: String,

    pub id: i64,

    pub title: String,

    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct UpdateSupergroup {
    #[serde(rename = "@type")]
    pub typ: String,

    pub id: i64,

    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct Message {
    #[serde(rename = "@type")]
    pub typ: String,

    pub id: i64,

    pub chat_id: i64,

    #[serde(flatten)]
    pub content: BTreeMap<String, Value>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct Messages {
    #[serde(rename = "@type")]
    pub typ: String,

    pub total_count: i32,

    pub messages: Vec<Message>,
}
