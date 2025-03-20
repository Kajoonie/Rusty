use poise::serenity_prelude as serenity;
use std::sync::Once;
use dotenv::dotenv;
use std::env;

mod commands;

use commands::general::{
    coingecko::coin::*,
    openai::{chat::*, imgen::*, question::*},
    ping::*,
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

static mut OPENAI_API_KEY: Option<String> = None;
static OPENAI_API_KEY_INIT: Once = Once::new();

fn set_openai_api_key() {
    OPENAI_API_KEY_INIT.call_once(|| unsafe {
        OPENAI_API_KEY = Some(env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set"));
    });
}

pub fn openai_api_key() -> String {
    unsafe { OPENAI_API_KEY.clone().unwrap() }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenv().ok();
    set_openai_api_key();

    let token = env::var("DISCORD_TOKEN").expect("Missing DISCORD_TOKEN");
    let intents = serenity::GatewayIntents::non_privileged();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                help(),
                register(),
                ping(),
                coin(),
                chat(),
                imgen(),
                question(),
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
        .await?;

    client.start().await.map_err(Into::into)
}
