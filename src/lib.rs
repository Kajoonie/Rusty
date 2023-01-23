use std::env;

use poise::serenity_prelude as serenity;

mod commands;
mod openai;

use commands::{
    admins::slow_mode::*,
    general::{imgen::*, ping::*, question::*},
};

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

pub struct Rusty {}

impl Rusty {
    fn discord_token() -> String {
        env::var("DISCORD_TOKEN").expect("Discord token not specified in env")
    }

    pub async fn start(&self) -> Result<(), shuttle_service::error::CustomError> {
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
            .token(Self::discord_token())
            .intents(serenity::GatewayIntents::non_privileged())
            .setup(|ctx, _, framework| {
                Box::pin(async move {
                    poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                    Ok(())
                })
            });

        framework.run().await.unwrap();

        Ok(())
    }
}

#[shuttle_service::async_trait]
impl shuttle_service::Service for Rusty {
    async fn bind(
        mut self: Box<Self>,
        _addr: std::net::SocketAddr,
    ) -> Result<(), shuttle_service::error::Error> {
        self.start().await?;

        Ok(())
    }
}

#[shuttle_service::main]
async fn init() -> Result<Rusty, shuttle_service::Error> {
    dotenv::dotenv().expect("Failed to load .env file");
    Ok(Rusty {})
}
