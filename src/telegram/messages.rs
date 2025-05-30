// This is free and unencumbered software released into the public domain.

use serde_json::Value;
use std::{collections::BTreeMap, string::String, vec::Vec};

#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "@type", rename_all = "camelCase")]
pub enum TdLibResponse {
    // Ok {
    //     #[serde(rename = "@extra")]
    //     extra: String,
    // },
    UpdateAuthorizationState {
        authorization_state: UpdateAuthState,
    },
    // Chats {
    //     #[serde(rename = "@extra")]
    //     extra: String,
    //     total_count: u64,
    //     chat_ids: Vec<i64>,
    // },
    UpdateSuperGroup {
        supergroup: UpdateSupergroup,
    },
    UpdateNewChat {
        chat: UpdateNewChat,
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
