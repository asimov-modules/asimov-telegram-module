// This is free and unencumbered software released into the public domain.

use asimov_telegram_module::telegram::{Client, Config};
use clientele::{
    StandardOptions,
    SysexitsError::{self, *},
    crates::clap::{self, Parser, Subcommand},
};
use miette::{Result, miette};
// use oxrdf::{Literal, NamedNode, Triple};

/// ASIMOV Telegram Cataloger
#[derive(Debug, Parser)]
#[command(name = "asimov-telegram-cataloger", long_about)]
struct Options {
    #[clap(flatten)]
    flags: StandardOptions,

    /// Available subjects: `chats`, `groups`, `members`, `users`
    #[clap(long, short, value_delimiter = ',')]
    subjects: Vec<String>,

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
    // Load environment variables from `.env`:
    clientele::dotenv().ok();

    tracing_subscriber::fmt::init();

    // Expand wildcards and @argfiles:
    let Ok(args) = clientele::args_os() else {
        return Ok(EX_USAGE);
    };

    // Parse command-line options:
    let options = Options::parse_from(&args);

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

    let config = Config {
        database_directory: data_dir.into(),
        api_id: std::env::var("API_ID").expect("API_ID must be set"),
        api_hash: std::env::var("API_HASH").expect("API_HASH must be set"),
    };
    let client = Client::new(config).unwrap().init().await.unwrap();

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
        // TODO: improve
        return Err(miette!(
            "Expecting login code: use the `verify-code` subcommand and provide the code sent to you by telegram"
        ));
    }

    if !client.is_authorised().await {
        // TODO: improve
        return Err(miette!(
            "Currently unauthorized: use the `send-code` subcommand to request a login code from telegram for your account. Then verify with `verify-code`"
        ));
    }

    // let mut ser = oxrdfio::RdfSerializer::from_format(oxrdfio::RdfFormat::Turtle)
    //     .with_prefix("know", "http://know.dev/")
    //     .unwrap()
    //     .for_writer(std::io::stdout());

    let subjects = if !options.subjects.is_empty() {
        options.subjects
    } else {
        vec![
            "chats".into(),
            "groups".into(),
            "members".into(),
            "users".into(),
        ]
    };

    let filter = asimov_telegram_module::jq::filter();

    if subjects.contains(&"chats".to_string()) {
        let chats = client.get_chats().await?;

        for (_id, chat) in chats {
            match filter.filter_json(chat) {
                // TODO: print as json or RDF?
                Ok(v) => println!("{v}"),
                Err(jq::JsonFilterError::NoOutput) => (),
                Err(err) => {
                    tracing::error!(?err);
                }
            }
        }

        // for (id, data) in chats {
        //     let sub = NamedNode::new(format!("http://know.dev/chat/#{}", id)).unwrap();
        //     if let Some(title) = data.title {
        //         let triple = Triple::new(
        //             sub.clone(),
        //             NamedNode::new("http://know.dev/title").unwrap(),
        //             Literal::new_simple_literal(title),
        //         );
        //         ser.serialize_triple(&triple).into_diagnostic()?;
        //     }
        //     if let Some(supergroup) = data.supergroup {
        //         let triple = Triple::new(
        //             sub.clone(),
        //             NamedNode::new("http://know.dev/supergroup").unwrap(),
        //             Literal::new_simple_literal(supergroup.to_string()), // TODO: typed literal
        //         );
        //         ser.serialize_triple(&triple).into_diagnostic()?;
        //     }
        // }
        //
    }

    if subjects.contains(&"groups".to_string()) {
        let supergroups = client.get_groups().await?;
        for (_id, supergroup) in supergroups {
            match filter.filter_json(supergroup) {
                // TODO: print as json or RDF?
                Ok(v) => println!("{v}"),
                Err(jq::JsonFilterError::NoOutput) => (),
                Err(err) => {
                    tracing::error!(?err);
                }
            }
        }

        // for (id, data) in supergroups {
        //     let sub = NamedNode::new(format!("http://know.dev/supergroup/#{}", id)).unwrap();
        //     for name in data.usernames {
        //         let triple = Triple::new(
        //             sub.clone(),
        //             NamedNode::new("http://know.dev/username").unwrap(),
        //             Literal::new_simple_literal(name),
        //         );
        //         ser.serialize_triple(&triple).into_diagnostic()?;
        //     }
        // }
    }

    if subjects.contains(&"members".to_string()) {
        let members = client.get_group_members(Some(200)).await?;
        tracing::debug!(
            len = members.values().map(|ms| ms.len()).sum::<usize>(),
            "Got members"
        );
        // TODO: this needs to print the group id too
        for (_id, group_members) in members {
            for member in group_members {
                match filter.filter_json(member) {
                    // TODO: print as json or RDF?
                    Ok(v) => println!("{v}"),
                    Err(jq::JsonFilterError::NoOutput) => (),
                    Err(err) => {
                        tracing::error!(?err);
                    }
                }
            }
            // for m in group_members {
            //     tracing::debug!(%id, %m, "Member");
            // }
        }
    }

    if subjects.contains(&"users".to_string()) {
        let users = client.get_users().await?;
        tracing::debug!(len = users.len(), "Got users");
        for (_id, user) in users {
            match filter.filter_json(user) {
                // TODO: print as json or RDF?
                Ok(v) => println!("{v}"),
                Err(jq::JsonFilterError::NoOutput) => (),
                Err(err) => {
                    tracing::error!(?err);
                }
            }
        }
        // for (id, u) in users {
        //     tracing::debug!(%id, %u, "User")
        // }
    }

    // ser.finish().into_diagnostic()?;

    Ok(EX_OK)
}
