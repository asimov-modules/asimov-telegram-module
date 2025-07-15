// This is free and unencumbered software released into the public domain.

use asimov_module::models::ModuleManifest;
use asimov_telegram_module::telegram::{Client, Config};
use clientele::{
    StandardOptions,
    SysexitsError::{self, *},
    crates::clap::{self, Parser},
};
use miette::{IntoDiagnostic, Result, miette};
// use oxrdf::{Literal, NamedNode, Triple};

/// ASIMOV Telegram Cataloger
#[derive(Debug, Parser)]
#[command(name = "asimov-telegram-cataloger", long_about)]
struct Options {
    #[clap(flatten)]
    flags: StandardOptions,

    /// Available subjects: `chats`, `groups`, `members`, `users`
    // #[clap(long, short, value_delimiter = ',')]
    // subjects: Vec<String>,

    /// Maximum amount of members per group to fetch.
    #[clap(long, default_value = "200")]
    max_members: usize,
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

    let client = Client::new(config).unwrap().init().await.unwrap();

    if !client.is_authorised().await {
        return Err(miette!("Unauthorized. Run `asimov module config telegram`"));
    }

    let filter = asimov_telegram_module::jq::filter();

    let chats = client.get_chats().await?;
    for (_id, chat) in chats {
        match filter.filter_json(chat) {
            Ok(filtered) => {
                if cfg!(feature = "pretty") {
                    colored_json::write_colored_json(&filtered, &mut std::io::stdout())
                        .into_diagnostic()?;
                    println!();
                } else {
                    println!("{filtered}");
                }
            }
            Err(jq::JsonFilterError::NoOutput) => (),
            Err(err) => tracing::error!(?err),
        }
    }

    Ok(EX_OK)
}
