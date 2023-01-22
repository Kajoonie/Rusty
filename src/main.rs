use poise::serenity_prelude as serenity;
use std::env;

use commands::{
    admins::slow_mode::*,
    general::{imgen::*, ping::*, question::*},
};

mod commands;
mod openai;

type Data = ();
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;
type CommandResult = Result<(), Error>;

#[poise::command(prefix_command, track_edits, slash_command)]
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

#[tokio::main]
async fn main() {
    // let token = if let Some(token) = secret_store.get("DISCORD_TOKEN") {
    //     token
    // } else {
    //     return Err(anyhow!("'DISCORD_TOKEN' was not found").into());
    // };
    dotenv::dotenv().expect("Failed to load .env file");
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

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
            ],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("!".into()),
                ..Default::default()
            },
            ..Default::default()
        })
        .token(token)
        .intents(serenity::GatewayIntents::non_privileged())
        .setup(|ctx, _, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(())
            })
        });

    framework.run().await.unwrap();
}
