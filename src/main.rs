use poise::serenity_prelude as serenity;
use dotenv::dotenv;
use std::env;
use songbird::SerenityInit;

mod commands;
mod database;
mod brave;

use commands::{
    ai::{
        chat::*,
        list_models::*,
        set_model::*,
        search::*,
        get_model::*,
    },
    coingecko::coin::*,
    general::ping::*,
    music::{
        play::*,
        queue::*,
        skip::*,
        stop::*,
        leave::*,
    },
};

type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;
type CommandResult = Result<(), Error>;

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
    dotenv().ok();

    // Initialize the SQLite database
    if let Err(e) = database::init_db() {
        eprintln!("Failed to initialize database: {}", e);
    }

    let token = env::var("DISCORD_TOKEN").expect("Missing DISCORD_TOKEN");
    // Add voice intent to allow voice functionality
    let intents = serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT | serenity::GatewayIntents::GUILD_VOICE_STATES;

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                // Default commands
                register(),
                help(),
                // General commands
                ping(),
                // AI-centric commands
                chat(),
                get_model(),
                list_models(),
                search(),
                set_model(),
                // Coingecko commands
                coin(),
                // Music commands
                play(),
                queue(),
                skip(),
                stop(),
                leave(),
            ],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {})
            })
        });

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework.build())
        // Register songbird with the client
        .register_songbird()
        .await?;

    client.start().await.map_err(Into::into)
}
