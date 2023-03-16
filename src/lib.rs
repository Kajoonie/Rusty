use std::sync::Once;

use poise::serenity_prelude as serenity;

mod commands;

use commands::{
    admins::slow_mode::*,
    general::{
        coingecko::coin::*,
        openai::{imgen::*, question::*, chat::*},
        ping::*,
    },
};
use shuttle_secrets::SecretStore;

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

async fn is_admin(ctx: Context<'_>) -> Result<bool, Error> {
    if let Some(guild_id) = ctx.guild_id() {
        for role in guild_id.member(ctx, ctx.author().id).await?.roles {
            if role.to_role_cached(ctx).map_or(false, |r| {
                r.has_permission(serenity::Permissions::ADMINISTRATOR)
            }) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[shuttle_service::main]
async fn init(
    #[shuttle_secrets::Secrets] secret_store: SecretStore,
) -> shuttle_service::ShuttlePoise<impl Send + Sync + 'static, impl Send + 'static> {
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
                //admin
                slow_mode(),
                //general
                ping(),
                question(),
                imgen(),
                chat(),
                coin(),
            ],
            ..Default::default()
        })
        .token(discord_token)
        .intents(serenity::GatewayIntents::non_privileged())
        .setup(|ctx, _, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(())
            })
        })
        .build()
        .await;

    match framework {
        Ok(f) => Ok(f),
        Err(e) => panic!("{e}"),
    }
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
