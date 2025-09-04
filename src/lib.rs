// This is free and unencumbered software released into the public domain.

#![no_std]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

use alloc::{format, vec::Vec};

pub mod jq;
pub mod shared;
pub mod telegram;

use miette::{Result, miette};

#[derive(Debug, PartialEq, Eq)]
pub enum FetchTarget {
    Chats,
    Chat { chat_id: i64 },
    ChatMembers { chat_id: i64 },
    ChatMessages { chat_id: i64 },
    UserInfo { user_id: i64 },
}

impl alloc::fmt::Display for FetchTarget {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use FetchTarget::*;
        match self {
            Chats => write!(f, "chat list"),
            Chat { .. } => write!(f, "chat info"),
            ChatMembers { .. } => write!(f, "chat member list"),
            ChatMessages { .. } => write!(f, "chat message list"),
            UserInfo { .. } => write!(f, "user info"),
        }
    }
}

pub fn parse_resource_url(url_str: &str) -> Result<FetchTarget> {
    let url: url::Url = url_str.parse().map_err(|e| miette!("Invalid URL: {e}"))?;

    if url.scheme() != "tg" {
        return Err(miette!("Unknown scheme `{}`, expected `tg`", url.scheme()));
    }

    // Handle both tg://host/path and tg:path formats
    let segments: Vec<&str> = if url.cannot_be_a_base() {
        // For tg:path format, parse the path manually
        let path = url.path();
        if path.is_empty() {
            return Err(miette!("Invalid URL: no path"));
        }
        path.split('/').filter(|s| !s.is_empty()).collect()
    } else {
        // For tg://host/path format, combine host and path segments
        let mut segments = Vec::new();

        // Add host as first segment if present
        if let Some(host) = url.host_str() {
            segments.push(host);
        }

        // Add path segments
        if let Some(path_segments) = url.path_segments() {
            segments.extend(path_segments.filter(|s| !s.is_empty()));
        }

        segments
    };

    match segments.as_slice() {
        ["chats"] | ["chat"] => Ok(FetchTarget::Chats),
        ["chats", chat_id] | ["chat", chat_id] => Ok(FetchTarget::Chat {
            chat_id: chat_id
                .parse()
                .map_err(|e| miette!("Invalid chat ID: {chat_id:?}: {e}"))?,
        }),
        ["chats", chat_id, "members"] | ["chat", chat_id, "members"] => {
            Ok(FetchTarget::ChatMembers {
                chat_id: chat_id
                    .parse()
                    .map_err(|e| miette!("Invalid chat ID: {chat_id:?}: {e}"))?,
            })
        }
        ["chats", chat_id, "messages"] | ["chat", chat_id, "messages"] => {
            Ok(FetchTarget::ChatMessages {
                chat_id: chat_id
                    .parse()
                    .map_err(|e| miette!("Invalid chat ID: {chat_id:?}: {e}"))?,
            })
        }
        ["users", user_id] | ["user", user_id] => Ok(FetchTarget::UserInfo {
            user_id: user_id
                .parse()
                .map_err(|e| miette!("Invalid user ID: {user_id:?}: {e}"))?,
        }),
        _ => Err(miette!("Unsupported URL format: {}", url_str)),
    }
}

#[cfg(test)]
mod tests {
    use std::{string::ToString as _, vec};

    use super::*;

    #[test]
    fn test_parse_fetch_url() {
        use FetchTarget::*;

        let test_cases = vec![
            ("tg://chat/12345", Chat { chat_id: 12345 }),
            ("tg://chats/12345", Chat { chat_id: 12345 }),
            ("tg:chat/12345", Chat { chat_id: 12345 }),
            ("tg:chats/12345", Chat { chat_id: 12345 }),
            ("tg://chat/12345/members", ChatMembers { chat_id: 12345 }),
            ("tg://chats/12345/members", ChatMembers { chat_id: 12345 }),
            ("tg:chat/12345/members", ChatMembers { chat_id: 12345 }),
            ("tg:chats/12345/members", ChatMembers { chat_id: 12345 }),
            ("tg://chat/12345/messages", ChatMessages { chat_id: 12345 }),
            ("tg://chats/12345/messages", ChatMessages { chat_id: 12345 }),
            ("tg:chat/12345/messages", ChatMessages { chat_id: 12345 }),
            ("tg:chats/12345/messages", ChatMessages { chat_id: 12345 }),
            ("tg://user/12345", UserInfo { user_id: 12345 }),
            ("tg://users/12345", UserInfo { user_id: 12345 }),
            ("tg:user/12345", UserInfo { user_id: 12345 }),
            ("tg:users/12345", UserInfo { user_id: 12345 }),
        ];

        for (url, expected) in test_cases {
            let result = parse_resource_url(url).unwrap();
            match (result, expected) {
                (Chat { chat_id: a }, Chat { chat_id: b }) => {
                    assert_eq!(a, b)
                }
                (ChatMembers { chat_id: a }, ChatMembers { chat_id: b }) => assert_eq!(a, b),
                (ChatMessages { chat_id: a }, ChatMessages { chat_id: b }) => assert_eq!(a, b),
                (UserInfo { user_id: a }, UserInfo { user_id: b }) => {
                    assert_eq!(a, b)
                }
                _ => panic!("Unexpected target type for URL: {url}"),
            }
        }

        let error_cases = vec![
            ("http://chat/12345", "Unknown scheme"),
            ("http://chats/12345", "Unknown scheme"),
            ("tg://chats/not_a_number", "Invalid chat ID"),
            ("tg://users/not_a_number", "Invalid user ID"),
            ("tg://unknown/format", "Unsupported URL format"),
        ];

        for (url, expected_error) in error_cases {
            let result = parse_resource_url(url);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains(expected_error));
        }
    }
}
