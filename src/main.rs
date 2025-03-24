use poise::serenity_prelude as serenity;
use dotenv::dotenv;
use std::env;
use tracing_subscriber::{EnvFilter, FmtSubscriber};

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
    // Initialize logging with debug level for our crate
    FmtSubscriber::builder()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("rusty=debug,warn"))
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
    if let Err(e) = database::init_db() {
        eprintln!("Failed to initialize database: {}", e);
    }

    let token = env::var("DISCORD_TOKEN").expect("Missing DISCORD_TOKEN");

    let intents = serenity::GatewayIntents::non_privileged() | serenity::GatewayIntents::MESSAGE_CONTENT | serenity::GatewayIntents::GUILD_VOICE_STATES;

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
        search(),
        set_model(),
        // Coingecko commands
        coin(),
    ];

    // Handle Music feature
    #[cfg(feature = "music")]
    {
        use commands::music::{
            play::*,
            queue::*,
            skip::*,
            stop::*,
            leave::*,
            pause::*,
            remove::*,
        };

        // Add music commands
        commands.extend(vec![
            play(),
            pause(),
            queue(),
            remove(),
            skip(),
            stop(),
            leave(),
        ]);
    }

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands,
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {})
            })
        });

    let framework_built = framework.build();
    
    // Create and run client
    let client_builder = serenity::ClientBuilder::new(token, intents)
        .framework(framework_built);
    
    #[cfg(feature = "music")]
    return run_with_music(client_builder).await;
    
    #[cfg(not(feature = "music"))]
    return run_without_music(client_builder).await;
}

// Only compiled when the music feature is enabled
#[cfg(feature = "music")]
async fn run_with_music(
    client_builder: serenity::ClientBuilder
) -> Result<(), Error> {
    // Required for music functionality
    use songbird::SerenityInit;
    
    let mut client = client_builder
        .register_songbird()
        .await?;
        
    client.start().await.map_err(Into::into)
}

// Only compiled when the music feature is disabled
#[cfg(not(feature = "music"))]
async fn run_without_music(
    client_builder: serenity::ClientBuilder
) -> Result<(), Error> {
    let mut client = client_builder.await?;
    client.start().await.map_err(Into::into)
}
