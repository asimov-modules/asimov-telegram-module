// This is free and unencumbered software released into the public domain.

use std::sync::Arc;

use asimov_telegram_module::telegram::{Client, Config};
use asimov_telegram_module::jq;
use clientele::{
    crates::clap::{self, Parser, Subcommand},
    StandardOptions,
    SysexitsError::{self, *},
};
use futures::StreamExt;
use miette::{miette, Result};
use serde_json::Value;
use tracing_subscriber::fmt;

/// ASIMOV Telegram Importer
#[derive(Debug, Parser)]
#[command(name = "asimov-telegram-importer", long_about)]
struct Options {
    #[clap(flatten)]
    flags: StandardOptions,

    #[clap(subcommand)]
    command: Option<Command>,
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    /// Ask Telegram to send a login code to `phone`
    SendCode { phone: String },
    /// Confirm code you received in Telegram
    VerifyCode { code: String },
}

#[tokio::main]
async fn main() -> Result<SysexitsError> {
    clientele::dotenv().ok();
    fmt::init();

    let Ok(args) = clientele::args_os() else { return Ok(EX_USAGE) };
    let options  = Options::parse_from(&args);

    if options.flags.version {
        println!("asimov-telegram {}", env!("CARGO_PKG_VERSION"));
        return Ok(EX_OK);
    }

    let Some(data_dir) =
        clientele::paths::xdg_data_home().map(|p| p.join("asimov-telegram-module"))
    else {
        return Err(miette!(
            "Unable to determine a directory for data. Neither $XDG_DATA_HOME nor $HOME available."
        ));
    };

    let cfg = Config {
        database_directory: data_dir.into(),
        api_id:   std::env::var("API_ID").expect("API_ID must be set"),
        api_hash: std::env::var("API_HASH").expect("API_HASH must be set"),
    };
    let client = Arc::new(Client::new(cfg).unwrap().init().await.unwrap());

    match &options.command {
        Some(Command::SendCode { phone })  => { client.send_auth_request(phone).await?; return Ok(EX_OK); }
        Some(Command::VerifyCode { code }) => { client.send_auth_code(code).await?;     return Ok(EX_OK); }
        None => {}
    }

    if client.is_need_code().await {
        return Err(miette!(
            "Expecting login code: use `verify-code` subcommand with the code you received."
        ));
    }
    if !client.is_authorised().await {
        return Err(miette!(
            "Unauthorised: run `send-code` then `verify-code` first."
        ));
    }

    let filter = jq::filter();
    let st = Arc::clone(&client).all_messages_stream();
    tokio::pin!(st);

    let mut total = 0u64;

    while let Some(res) = st.next().await {
        match res {
            Ok(msg) => {
                if let Some(Value::Object(content)) = msg.other.get("content") {
                    if let Some(Value::Object(text)) = content.get("text") {
                        if let Some(Value::String(text_str)) = text.get("text") {
                            println!(">> {}", text_str);
                        }
                    }
                }

                let value = serde_json::to_value(&msg)
                    .map_err(|e| miette!("Failed to serialize message: {}", e))?;

                match filter.filter_json(value) {
                    Ok(filtered) => {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&filtered)
                                .map_err(|e| miette!("Failed to serialize filtered: {}", e))?
                        );
                        total += 1;
                        tracing::info!(
                chat_id    = msg.chat_id,
                message_id = msg.id,
                "Processed message"
            );
                    }
                    Err(jq::JsonFilterError::NoOutput) => (),
                    Err(err) => tracing::error!(?err, "Filter failed"),
                }
            }
            Err(e) => tracing::warn!("TDLib error: {e}"),
        }
    }

    println!("Processed {total} messages across all chats");
    Ok(EX_OK)
}
