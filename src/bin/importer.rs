// This is free and unencumbered software released into the public domain.

use asimov_module::models::ModuleManifest;
use asimov_telegram_module::telegram::{Client, Config};
use clientele::{
    StandardOptions,
    SysexitsError::{self, *},
    crates::clap::{self, Parser},
};
use miette::{IntoDiagnostic, Result, miette};
use serde_json::Value;
use std::sync::Arc;

#[derive(Debug, Parser)]
#[command(
    name = "asimov-telegram-importer",
    long_about = "ASIMOV Telegram Importer"
)]
struct Options {
    #[clap(flatten)]
    flags: StandardOptions,

    /// The maximum number of resources to list.
    #[arg(value_name = "COUNT", short = 'n', long)]
    limit: Option<usize>,

    /// The output format.
    #[arg(value_name = "FORMAT", short = 'o', long)]
    output: Option<String>,

    resource: String,
}

#[derive(Debug, PartialEq, Eq)]
enum FetchTarget {
    Chat { chat_id: i64 },
    ChatMembers { chat_id: i64 },
    ChatMessages { chat_id: i64 },
    UserInfo { user_id: i64 },
}

#[tokio::main]
async fn main() -> Result<SysexitsError> {
    // Load environment variables from `.env`:
    clientele::dotenv().ok();

    let Ok(args) = clientele::args_os() else {
        return Ok(EX_USAGE);
    };
    let options = Options::parse_from(&args);

    asimov_module::init_tracing_subscriber(&options.flags).expect("failed to initialize logging");

    if options.flags.version {
        println!("asimov-telegram-importer {}", env!("CARGO_PKG_VERSION"));
        return Ok(EX_OK);
    }

    let Some(data_dir) =
        clientele::paths::xdg_data_home().map(|p| p.join("asimov-telegram-module"))
    else {
        return Err(miette!(
            "Unable to determine a directory for data. Neither $XDG_DATA_HOME nor $HOME available."
        ));
    };

    let manifest = ModuleManifest::read_manifest("telegram").unwrap();

    let api_id = manifest
        .variable("API_ID", None)
        .expect("Missing API_ID. Run `asimov module config telegram`");
    let api_hash = manifest
        .variable("API_HASH", None)
        .expect("Missing API_HASH. Run `asimov module config telegram`");
    let encryption_key = asimov_telegram_module::telegram::get_or_create_encryption_key()?;

    let config = Config {
        database_directory: data_dir.into(),
        api_id,
        api_hash,
        encryption_key,
    };

    let client = Arc::new(Client::new(config).unwrap().init().await.unwrap());

    if !client.is_authorised().await {
        return Err(miette!("Unauthorized. Run `asimov module config telegram`"));
    }

    let filter = asimov_telegram_module::jq::filter();

    let print = |v: &Value| -> Result<()> {
        if cfg!(feature = "pretty") {
            colored_json::write_colored_json(&v, &mut std::io::stdout()).into_diagnostic()?;
            println!();
        } else {
            println!("{v}");
        }
        Ok(())
    };

    match parse_fetch_url(&options.resource)? {
        FetchTarget::Chat { chat_id } => {
            let info = client.get_chat_info(chat_id).await?;
            match filter.filter_json(info) {
                Ok(filtered) => print(&filtered)?,
                Err(jq::JsonFilterError::NoOutput) => (),
                Err(err) => tracing::error!(?err, "Filter failed"),
            }
        }
        FetchTarget::ChatMembers { chat_id } => {
            let users = client.get_chat_members(chat_id, options.limit).await?;
            for user in users {
                match filter.filter_json(user) {
                    Ok(filtered) => print(&filtered)?,
                    Err(jq::JsonFilterError::NoOutput) => (),
                    Err(err) => tracing::error!(?err, "Filter failed"),
                }
            }
        }
        FetchTarget::ChatMessages { chat_id } => {
            let msgs = client
                .get_chat_history(chat_id, None, options.limit)
                .await?;

            for msg in msgs {
                let msg = serde_json::to_value(msg).into_diagnostic()?;
                match filter.filter_json(msg) {
                    Ok(filtered) => print(&filtered)?,
                    Err(jq::JsonFilterError::NoOutput) => (),
                    Err(err) => tracing::error!(?err, "Filter failed"),
                }
            }
        }
        FetchTarget::UserInfo { user_id } => {
            let user = client.get_user(user_id).await?;
            match filter.filter_json(user) {
                Ok(filtered) => print(&filtered)?,
                Err(jq::JsonFilterError::NoOutput) => (),
                Err(err) => tracing::error!(?err, "Filter failed"),
            }
        }
    }

    Ok(EX_OK)
}

fn parse_fetch_url(url_str: &str) -> Result<FetchTarget> {
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
        ["chat", chat_id] => Ok(FetchTarget::Chat {
            chat_id: chat_id
                .parse()
                .map_err(|e| miette!("Invalid chat ID: {chat_id:?}: {e}"))?,
        }),
        ["chat", chat_id, "members"] => Ok(FetchTarget::ChatMembers {
            chat_id: chat_id
                .parse()
                .map_err(|e| miette!("Invalid chat ID: {chat_id:?}: {e}"))?,
        }),
        ["chat", chat_id, "messages"] => Ok(FetchTarget::ChatMessages {
            chat_id: chat_id
                .parse()
                .map_err(|e| miette!("Invalid chat ID: {chat_id:?}: {e}"))?,
        }),
        ["user", user_id] => Ok(FetchTarget::UserInfo {
            user_id: user_id
                .parse()
                .map_err(|e| miette!("Invalid user ID: {user_id:?}: {e}"))?,
        }),
        _ => Err(miette!("Unsupported URL format: {}", url_str)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fetch_url() {
        let test_cases = vec![
            ("tg://chat/12345", FetchTarget::Chat { chat_id: 12345 }),
            ("tg:chat/12345", FetchTarget::Chat { chat_id: 12345 }),
            (
                "tg://chat/12345/members",
                FetchTarget::ChatMembers { chat_id: 12345 },
            ),
            (
                "tg:chat/12345/members",
                FetchTarget::ChatMembers { chat_id: 12345 },
            ),
            (
                "tg://chat/12345/messages",
                FetchTarget::ChatMessages { chat_id: 12345 },
            ),
            (
                "tg:chat/12345/messages",
                FetchTarget::ChatMessages { chat_id: 12345 },
            ),
            ("tg://user/12345", FetchTarget::UserInfo { user_id: 12345 }),
            ("tg:user/12345", FetchTarget::UserInfo { user_id: 12345 }),
        ];

        for (url, expected) in test_cases {
            let result = parse_fetch_url(url).unwrap();
            match (result, expected) {
                (FetchTarget::Chat { chat_id: a }, FetchTarget::Chat { chat_id: b }) => {
                    assert_eq!(a, b)
                }
                (
                    FetchTarget::ChatMembers { chat_id: a },
                    FetchTarget::ChatMembers { chat_id: b },
                ) => assert_eq!(a, b),
                (
                    FetchTarget::ChatMessages { chat_id: a },
                    FetchTarget::ChatMessages { chat_id: b },
                ) => assert_eq!(a, b),
                (FetchTarget::UserInfo { user_id: a }, FetchTarget::UserInfo { user_id: b }) => {
                    assert_eq!(a, b)
                }
                _ => panic!("Unexpected target type for URL: {}", url),
            }
        }

        let error_cases = vec![
            ("http://chat/12345", "Unknown scheme"),
            ("tg://chat/not_a_number", "Invalid chat ID"),
            ("tg://user/not_a_number", "Invalid user ID"),
            ("tg://unknown/format", "Unsupported URL format"),
        ];

        for (url, expected_error) in error_cases {
            let result = parse_fetch_url(url);
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains(expected_error));
        }
    }
}
