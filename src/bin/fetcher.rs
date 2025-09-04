// This is free and unencumbered software released into the public domain.

use asimov_telegram_module::{
    FetchTarget,
    telegram::{Client, Config},
};
use clientele::{
    StandardOptions,
    SysexitsError::{self, *},
    crates::clap::{self, Parser},
};
use miette::{Result, miette};
use std::sync::Arc;

use asimov_telegram_module::{parse_resource_url, shared};

#[derive(Debug, Parser)]
#[command(
    name = "asimov-telegram-fetcher",
    long_about = "ASIMOV Telegram Fetcher"
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
        println!("asimov-telegram-fetcher {}", env!("CARGO_PKG_VERSION"));
        return Ok(EX_OK);
    }

    let target_resource = parse_resource_url(&options.resource)?;

    let data_dir = shared::get_data_dir()?;
    let api_id = obfstr::obfstring!(env!("ASIMOV_TELEGRAM_API_ID"));
    let api_hash = obfstr::obfstring!(env!("ASIMOV_TELEGRAM_API_HASH"));
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

    match target_resource {
        FetchTarget::Chat { chat_id } => {
            let info = client.get_chat_info(chat_id).await?;
            match filter.filter_json(info) {
                Ok(filtered) => println!("{filtered}"),
                Err(jq::JsonFilterError::NoOutput) => (),
                Err(err) => tracing::error!(?err, "Filter failed"),
            }
        }
        FetchTarget::UserInfo { user_id } => {
            let user = client.get_user(user_id).await?;
            match filter.filter_json(user) {
                Ok(filtered) => println!("{filtered}"),
                Err(jq::JsonFilterError::NoOutput) => (),
                Err(err) => tracing::error!(?err, "Filter failed"),
            }
        }
        target => {
            // FetchTarget::Chats, FetchTarget::ChatMembers, FetchTarget::ChatMessages
            return Err(miette!(
                "{target} is not a valid target resource for fetcher"
            ));
        }
    }

    Ok(EX_OK)
}
