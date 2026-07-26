mod commands;
mod util;

use std::env;

use poise::serenity_prelude as serenity;
use tracing::{error, info};

use crate::commands::backup::backup;
use crate::commands::kill_process::kill_process;
use crate::commands::list_desktop::list_desktop;
use crate::commands::list_process::list_process;
use crate::commands::list_saves::list_saves;
use crate::commands::read_config::read_config;
use crate::commands::restart_server::restart_server;
use crate::commands::restore_this::{restore_this, restore_this_menu};
use crate::commands::run_desktop_shortcut::run_desktop_shortcut;
use crate::commands::screenshot::screenshot;

/// Shared state passed to every command invocation.
pub struct Data {}
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

/// Show a list of commands, or detailed help for a single command.
#[poise::command(prefix_command, slash_command)]
pub async fn help(
    ctx: Context<'_>,
    #[description = "Command to show help for"] command: Option<String>,
) -> Result<(), Error> {
    poise::builtins::help(
        ctx,
        command.as_deref(),
        poise::builtins::HelpConfiguration::default(),
    )
    .await?;
    Ok(())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let intents = serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::DIRECT_MESSAGES
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                help(),
                restart_server(),
                screenshot(),
                list_process(),
                kill_process(),
                list_desktop(),
                list_saves(),
                run_desktop_shortcut(),
                read_config(),
                backup(),
                restore_this(),
                restore_this_menu(),
            ],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("!F".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                info!("Connected as {}", ready.user.name);
                Ok(Data {})
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(&token, intents)
        .framework(framework)
        .await
        .expect("Err creating client");

    let shard_manager = client.shard_manager.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not register ctrl+c handler");
        shard_manager.shutdown_all().await;
    });

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}
