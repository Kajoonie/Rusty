use ::serenity::all::ClientBuilder;
use dotenv::dotenv;
use poise::serenity_prelude as serenity;
use regex::Regex;
use std::{
    env,
    sync::{Arc, LazyLock},
    time::Duration,
};
use tracing::debug;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

mod commands;
mod events;
mod utils;

use commands::{
    ai::{chat::*, get_model::*, list_models::*, set_model::*},
    coingecko::coin::*,
    general::ping::*,
};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;
type CommandResult = Result<(), Error>;

pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

// Define the user data type we'll be using in our bot
struct Data {} // User data, which is stored and accessible in all command invocations

#[poise::command(slash_command, category = "General")]
async fn help(
    ctx: Context<'_>,
    #[description = "Specific command to show help about"]
    #[autocomplete = "poise::builtins::autocomplete_command"]
    command: Option<String>,
) -> CommandResult {
    poise::builtins::help(
        ctx,
        command.as_deref(),
        poise::builtins::HelpConfiguration {
            show_context_menu_commands: true,
            ..Default::default()
        },
    )
    .await
    .map_err(|e| e.into())
}

#[poise::command(prefix_command, hide_in_help)]
async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx)
        .await
        .map_err(|e| e.into())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize logging with debug level for our crate
    FmtSubscriber::builder()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("rusty=debug,warn")),
        )
        .with_thread_ids(true)
        .with_line_number(true)
        .with_file(true)
        .with_target(true)
        .with_ansi(true)
        .pretty()
        .init();

    dotenv().ok();

    // Initialize the SQLite database
    if let Err(e) = utils::database::init_db() {
        eprintln!("Failed to initialize database: {}", e);
    }

    let token = env::var("DISCORD_TOKEN").expect("Missing DISCORD_TOKEN");

    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_VOICE_STATES;

    // Create a vector to hold our commands
    let mut commands = vec![
        // Default commands
        register(),
        help(),
        // General commands
        ping(),
        // AI-centric commands
        chat(),
        get_model(),
        list_models(),
        set_model(),
        // Coingecko commands
        coin(),
    ];

    // Handle brave search feature
    #[cfg(feature = "brave_search")]
    {
        use commands::ai::search::*;

        commands.extend(vec![search()]);
    }

    // Handle Music feature
    #[cfg(feature = "music")]
    {
        use commands::music::{autoplay::*, play::*, remove::*};

        // Add music commands
        commands.extend(vec![autoplay(), play(), remove()]);
    }

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands,
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("~".into()),
                edit_tracker: Some(Arc::new(poise::EditTracker::for_timespan(
                    Duration::from_secs(3600),
                ))),
                additional_prefixes: vec![poise::Prefix::Regex(
                    Regex::new(r"(?i)\brusty\b,?").unwrap(),
                )],
                ..Default::default()
            },
            pre_command: |ctx| {
                Box::pin(async move {
                    debug!("Executing command {}...", ctx.command().qualified_name);
                })
            },
            post_command: |ctx| {
                Box::pin(async move {
                    debug!("Executed command {}!", ctx.command().qualified_name);
                })
            },
            event_handler: |ctx, event, framework, data| {
                Box::pin(async move { events::handle_event(ctx, event, framework, data).await })
            },
            ..Default::default()
        })
        .setup(|_ctx, _ready, _framework| Box::pin(async move { Ok(Data {}) }));

    let client_builder = ClientBuilder::new(token, intents).framework(framework.build());

    build_and_start_client(client_builder).await
}

async fn build_and_start_client(client_builder: serenity::ClientBuilder) -> Result<(), Error> {
    #[cfg(feature = "music")]
    {
        use songbird::SerenityInit;

        let mut client = client_builder.register_songbird().await?;
        client.start().await.map_err(Into::into)
    }

    #[cfg(not(feature = "music"))]
    {
        let mut client = client_builder.await?;
        client.start().await.map_err(Into::into)
    }
}
