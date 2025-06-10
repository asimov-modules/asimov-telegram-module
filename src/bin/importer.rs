// This is free and unencumbered software released into the public domain.

use std::sync::Arc;

use clientele::{
    crates::clap::{self, Parser, Subcommand},
    StandardOptions,
    SysexitsError::{self, *},
};
use futures::StreamExt;
use miette::{miette, Result};

use asimov_telegram_module::telegram::{Client, Config};

#[derive(Debug, Parser)]
#[command(name = "asimov-telegram-importer", long_about = "ASIMOV Telegram Importer")]
struct Options {
    #[clap(flatten)]
    flags: StandardOptions,

    #[clap(subcommand)]
    command: Option<Command>,
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    SendCode { phone: String },
    VerifyCode { code: String },
}

#[tokio::main]
async fn main() -> Result<SysexitsError> {
    clientele::dotenv().ok();
    tracing_subscriber::fmt::init();

    let Ok(args) = clientele::args_os() else { return Ok(EX_USAGE) };
    let options = Options::parse_from(&args);

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

    let config = Config {
        database_directory: data_dir.into(),
        api_id: std::env::var("API_ID").expect("API_ID must be set"),
        api_hash: std::env::var("API_HASH").expect("API_HASH must be set"),
    };

    let client = Arc::new(Client::new(config).unwrap().init().await.unwrap());

    match options.command {
        Some(Command::SendCode { phone }) => {
            client.send_auth_request(&phone).await?;
            return Ok(EX_OK);
        }
        Some(Command::VerifyCode { code }) => {
            client.send_auth_code(&code).await?;
            return Ok(EX_OK);
        }
        None => (),
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

    let filter = asimov_telegram_module::jq::filter();
    let st = Arc::clone(&client).all_messages_stream();
    tokio::pin!(st);

    let mut total = 0u64;

    while let Some(res) = st.next().await {
        match res {
            Ok(msg) => {
                let chat_id = msg["chat_id"].as_i64().unwrap_or(0);

                match filter.filter_json(msg.clone()) {
                    Ok(filtered) => {
                        println!("{filtered}");
                        total += 1;
                        tracing::debug!(
                            chat_id,
                            message_id = msg["id"].as_i64().unwrap_or(0),
                            "Processed message"
                        );
                    }
                    Err(jq::JsonFilterError::NoOutput) => (),
                    Err(err) => tracing::error!(?err, "Filter failed"),
                }

            }
            Err(e) => tracing::error!("TDLib error: {e}"),
        }
    }

    tracing::debug!("Processed {total} messages across all chats");
    Ok(EX_OK)
}