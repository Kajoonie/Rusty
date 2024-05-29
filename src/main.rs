use poise::serenity_prelude as serenity;
use ::serenity::all::{ClientBuilder, GatewayIntents};
use std::sync::Once;

mod commands;

use commands::general::{
        coingecko::coin::*,
        openai::{chat::*, imgen::*, question::*},
        ping::*,
    };
use shuttle_serenity::ShuttleSerenity;
use shuttle_runtime::SecretStore;

type Data = ();
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;
type CommandResult = Result<(), Error>;

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

#[shuttle_runtime::main]
async fn main(#[shuttle_runtime::Secrets] secret_store: SecretStore) -> ShuttleSerenity {
    set_openai_api_key(&secret_store);

    let discord_token = secret_store
        .get("DISCORD_TOKEN")
        .expect("DISCORD_TOKEN secret not present");

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                //helpers
                register(),
                help(),
                //general
                ping(),
                question(),
                imgen(),
                chat(),
                coin(),
            ],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(())
            })
        })
        .build();

    let client = ClientBuilder::new(discord_token, GatewayIntents::non_privileged())
        .framework(framework)
        .await
        .map_err(shuttle_runtime::CustomError::new)?;

    Ok(client.into())
}

static mut OPENAI_API_KEY: Option<String> = None;
static OPENAI_API_KEY_INIT: Once = Once::new();

fn set_openai_api_key(secret_store: &SecretStore) {
    unsafe {
        OPENAI_API_KEY_INIT.call_once(|| {
            OPENAI_API_KEY = secret_store.get("OPENAI_API_KEY");
        })
    }
}

fn openai_api_key() -> String {
    unsafe { OPENAI_API_KEY.clone().unwrap() }
}
