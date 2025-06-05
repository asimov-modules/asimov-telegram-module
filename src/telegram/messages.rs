// This is free and unencumbered software released into the public domain.

use serde_json::Value;
use std::{collections::BTreeMap, string::String, vec::Vec};

#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(tag = "@type", rename_all = "camelCase")]
pub enum TdLibResponse {
    UpdateAuthorizationState {
        authorization_state: UpdateAuthState,
    },
    UpdateNewChat {
        chat: UpdateNewChat,
    },
    #[serde[rename = "updateBasicGroup"]]
    UpdateBasicGroup {
        #[serde(rename = "basic_group")]
        basicgroup: UpdateBasicgroup,
    },
    #[serde[rename = "updateSupergroup"]] // note lowercase `g` in comparison to basicgroup
    UpdateSupergroup {
        supergroup: UpdateSupergroup,
    },
    UpdateUser {
        user: UpdateUser,
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
    pub id: i64,

    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct UpdateBasicgroup {
    pub id: i64,

    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct UpdateSupergroup {
    pub id: i64,

    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub struct UpdateUser {
    pub id: i64,

    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Message {
    #[serde(rename = "@type")]
    pub typ: String,

    #[serde(default)]
    pub id: i64,

    #[serde(default)]
    pub chat_id: i64,

    #[serde(flatten)]
    pub other: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Messages {
    #[serde(rename = "@type")]
    pub typ: String,

    pub total_count: Option<i32>,

    pub messages: Vec<Message>,
}
