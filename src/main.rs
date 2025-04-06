use ::serenity::all::ClientBuilder;
use dotenv::dotenv;
use poise::serenity_prelude as serenity;
use regex::Regex;
use std::{
    env,
    process::Command,
    sync::{Arc, LazyLock},
    time::Duration,
};
use tracing::debug;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

/// Module containing all bot commands.
mod commands;
/// Module handling Discord gateway events.
mod events;
/// Module for utility functions and shared resources.
mod utils;

use commands::{
    ai::{chat::*, get_model::*, list_models::*, set_model::*},
    coingecko::coin::*,
    general::ping::*,
    music::remove::remove,
};

/// Custom error type for handling various errors across the bot.
type Error = Box<dyn std::error::Error + Send + Sync>;
/// Custom context type alias using the bot's `Data` and `Error` types.
type Context<'a> = poise::Context<'a, Data, Error>;
/// Custom result type alias for command functions.
type CommandResult = Result<(), Error>;

/// Lazily initialized static HTTP client for making requests.
pub static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

/// Struct to hold shared data accessible across commands and events.
/// Currently empty, but can be expanded as needed.
struct Data {}

/// Displays help information for commands.
///
/// Uses poise's built-in help command functionality.
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

/// Registers application commands (slash commands) with Discord.
///
/// Hidden from the help menu, typically run once by the bot owner.
#[poise::command(prefix_command, hide_in_help)]
async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx)
        .await
        .map_err(|e| e.into())
}

#[cfg(feature = "music")]
/// Checks if `yt-dlp` is installed and executable.
///
/// Panics if `yt-dlp --version` fails, as it's required for the music feature.
/// This function is only compiled if the `music` feature is enabled.
fn check_ytdlp() {
    // First, verify yt-dlp is working
    let output = Command::new("yt-dlp")
        .arg("--version")
        .output()
        .expect("Failed to execute `yt-dlp --version`");

    if !output.status.success() {
        panic!("yt-dlp is not properly installed");
    }

    debug!(
        "yt-dlp version: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// The main entry point of the bot application.
#[tokio::main]
async fn main() -> Result<(), Error> {
    // Initialize the tracing subscriber for logging.
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

    // Load environment variables from a .env file if present.
    dotenv().ok();

    // Initialize the SQLite database connection.
    if let Err(e) = utils::database::init_db() {
        eprintln!("Failed to initialize database: {}", e);
    }

    // Retrieve the Discord bot token from environment variables.
    let token = env::var("DISCORD_TOKEN").expect("Missing DISCORD_TOKEN");

    // Define the necessary gateway intents for the bot.
    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_VOICE_STATES;

    // Initialize a vector to store all available commands.
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

    // Conditionally add the 'search' command if the 'brave_search' feature is enabled.
    #[cfg(feature = "brave_search")]
    {
        use commands::ai::search::*;

        commands.extend(vec![search()]);
    }

    // Conditionally add music-related commands and perform checks if the 'music' feature is enabled.
    #[cfg(feature = "music")]
    {
        check_ytdlp();

        use commands::music::{autoplay::*, play::*};

        // Add music commands
        commands.extend(vec![autoplay(), play(), remove()]);
    }

    // Configure and build the poise framework.
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
            ..Default::default()
        })
        .setup(|_ctx, _ready, _framework| Box::pin(async move { Ok(Data {}) }));

    // Build the Serenity client builder with the token, intents, event handler, and framework.
    let client_builder = ClientBuilder::new(token, intents)
        .event_handler(events::Handler)
        .framework(framework.build());

    build_and_start_client(client_builder).await
}

/// Builds the Serenity client and starts the bot.
///
/// Conditionally registers songbird if the `music` feature is enabled.
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
