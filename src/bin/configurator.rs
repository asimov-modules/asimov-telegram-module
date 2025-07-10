// This is free and unencumbered software released into the public domain.

use asimov_module::models::ModuleManifest;
use asimov_telegram_module::telegram::{Client, Config};
use clientele::{
    StandardOptions,
    SysexitsError::{self, *},
    crates::clap::{self, Parser},
};
use miette::{Result, miette};
use std::io::{BufRead, Write};

/// ASIMOV Telegram Configurator
#[derive(Debug, Parser)]
#[command(name = "asimov-telegram-configurator", long_about)]
struct Options {
    #[clap(flatten)]
    flags: StandardOptions,
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

    if client.is_authorised().await {
        return Ok(EX_OK);
    }

    let mut stdout = std::io::stdout().lock();
    let mut lines = std::io::stdin().lock().lines();

    let phone = loop {
        write!(&mut stdout, "Enter phone: ").unwrap();
        stdout.flush().unwrap();
        if let Some(Ok(phone)) = lines.next()
            && !phone.is_empty()
        {
            break phone;
        }
    };

    client.send_auth_request(&phone).await?;

    let code = loop {
        write!(&mut stdout, "Enter code: ").unwrap();
        stdout.flush().unwrap();
        if let Some(Ok(code)) = lines.next()
            && !code.is_empty()
        {
            break code;
        }
    };
    client.send_auth_code(&code).await?;

    if !client.is_authorised().await {
        // TODO: improve
        return Err(miette!("Something went wrong, still unauthorized"));
    }

    Ok(EX_OK)
}
