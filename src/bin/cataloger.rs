// This is free and unencumbered software released into the public domain.

use asimov_telegram_module::{
    FetchTarget, parse_resource_url,
    telegram::{Client, Config},
};
use clientele::{
    StandardOptions,
    SysexitsError::{self, *},
    crates::clap::{self, Parser},
};
use futures::StreamExt as _;
use miette::{IntoDiagnostic as _, Result, miette};
// use oxrdf::{Literal, NamedNode, Triple};

use asimov_telegram_module::shared;

/// ASIMOV Telegram Cataloger
#[derive(Debug, Parser)]
#[command(name = "asimov-telegram-cataloger", long_about)]
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

    // Expand wildcards and @argfiles:
    let Ok(args) = clientele::args_os() else {
        return Ok(EX_USAGE);
    };

    // Parse command-line options:
    let options = Options::parse_from(&args);

    asimov_module::init_tracing_subscriber(&options.flags).expect("failed to initialize logging");

    // Print the version, if requested:
    if options.flags.version {
        println!("asimov-telegram {}", env!("CARGO_PKG_VERSION"));
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

    let client = Client::new(config).unwrap().init().await.unwrap();

    if !client.is_authorised().await {
        return Err(miette!("Unauthorized. Run `asimov module config telegram`"));
    }

    let filter = asimov_telegram_module::jq::filter();

    match target_resource {
        FetchTarget::Chats => {
            let chats = client
                .get_chats()
                .await?
                .into_iter()
                .take(options.limit.unwrap_or(usize::MAX));
            for (_id, chat) in chats {
                match filter.filter_json(chat) {
                    Ok(filtered) => println!("{filtered}"),
                    Err(jq::JsonFilterError::NoOutput) => (),
                    Err(err) => tracing::error!(?err),
                }
            }
        }
        FetchTarget::ChatMembers { chat_id } => {
            let mut users = client
                .get_chat_members(chat_id, options.limit)
                .await?
                .boxed();

            while let Some(user) = users.next().await {
                let user = user?;
                match filter.filter_json(user) {
                    Ok(filtered) => println!("{filtered}"),
                    Err(jq::JsonFilterError::NoOutput) => (),
                    Err(err) => tracing::error!(?err, "Filter failed"),
                }
            }
        }
        FetchTarget::ChatMessages { chat_id } => {
            let mut msgs = client
                .get_chat_history(chat_id, None, options.limit)
                .await?
                .boxed();

            while let Some(msg) = msgs.next().await {
                let msg = serde_json::to_value(msg?).into_diagnostic()?;
                match filter.filter_json(msg) {
                    Ok(filtered) => println!("{filtered}"),
                    Err(jq::JsonFilterError::NoOutput) => (),
                    Err(err) => tracing::error!(?err, "Filter failed"),
                }
            }
        }
        target => {
            // just FetchTarget::Chats
            return Err(miette!(
                "{target} is not a valid target resource for cataloger"
            ));
        }
    }

    Ok(EX_OK)
}
