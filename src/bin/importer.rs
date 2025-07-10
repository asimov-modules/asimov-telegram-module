// This is free and unencumbered software released into the public domain.

use asimov_module::models::ModuleManifest;
use asimov_telegram_module::telegram::{Client, Config};
use clientele::{
    StandardOptions,
    SysexitsError::{self, *},
    crates::clap::{self, Parser},
};
use futures::StreamExt;
use miette::{Result, miette};
use std::sync::Arc;

#[derive(Debug, Parser)]
#[command(
    name = "asimov-telegram-importer",
    long_about = "ASIMOV Telegram Importer"
)]
struct Options {
    #[clap(flatten)]
    flags: StandardOptions,
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
