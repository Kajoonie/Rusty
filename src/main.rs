use ::serenity::all::ClientBuilder;
use dotenv::dotenv;
use poise::serenity_prelude::{
    self as serenity, // Keep serenity import
    CreateInteractionResponse, CreateInteractionResponseFollowup, // Keep response types
    CreateInteractionResponseMessage, // Add message builder type
    Interaction, // Keep Interaction
    // Remove ActionRowComponentKind, ComponentInteractionDataKind, PoiseContext
};
use regex::Regex;
use tracing::{error, info, warn}; // Add tracing macros
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
    // Add imports for music functionality needed in the event handler
    music::{
        play::process_play_request, // Import the refactored function
        utils::{
            embedded_messages, // Import for sending messages
            music_manager::{MusicError, MusicManager}, // Import Music types
        },
    },
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
                Box::pin(async move {
                    match event {
                        serenity::FullEvent::InteractionCreate { interaction } => {
                            match interaction {
                                Interaction::Modal(mut modal_interaction) => {
                                    info!(
                                        "Received modal submission with custom_id: {}",
                                        modal_interaction.data.custom_id
                                    );
                                    if modal_interaction.data.custom_id == "music_search_modal" {
                                        // --- Extract Data ---
                                        let query = modal_interaction
                                            .data
                                            .components
                                            .get(0) // First action row (ModalInteractionDataComponent)
                                            // Use and_then to chain the next get call
                                            .and_then(|row_data| row_data.components.get(0)) // First ActionRow in row_data
                                            .and_then(|action_row| action_row.components.get(0)) // First ActionRowComponent in ActionRow
                                            .and_then(|component| match component {
                                                // Match directly on ActionRowComponent
                                                serenity::ActionRowComponent::InputText(text_input) => Some(&text_input.value),
                                                _ => None,
                                            })
                                            .cloned::<String>() // Add type hint to resolve ambiguity
                                            .unwrap_or_default();

                                        if query.is_empty() {
                                            error!("Extracted empty query from music search modal");
                                            let reply = embedded_messages::generic_error("Search query was empty.");
                                            if let Err(e) = modal_interaction
                                                // Use builder pattern for response message
                                                .create_response(&ctx.http, CreateInteractionResponse::Message(
                                                    CreateInteractionResponseMessage::new().add_embed(reply.embeds.remove(0)).ephemeral(true)
                                                ))
                                                .await
                                            {
                                                error!("Failed to send modal error response: {}", e);
                                            }
                                            return Ok(()); // Return early on error
                                        }

                                        let guild_id = match modal_interaction.guild_id {
                                            Some(id) => id,
                                            None => {
                                                error!("Modal interaction without guild_id");
                                                let reply =
                                                    embedded_messages::generic_error("Command must be used in a server.");
                                                if let Err(e) = modal_interaction
                                                    // Use builder pattern for response message
                                                    .create_response(&ctx.http, CreateInteractionResponse::Message(
                                                        CreateInteractionResponseMessage::new().add_embed(reply.embeds.remove(0)).ephemeral(true)
                                                    ))
                                                    .await
                                                {
                                                    error!("Failed to send modal error response: {}", e);
                                                }
                                                return Ok(()); // Return early on error
                                            }
                                        };
                                        let user_id = modal_interaction.user.id;

                                        // --- Get Voice Channel ---
                                        // Need full context for get_user_voice_channel
                                        let voice_channel_id = match MusicManager::get_user_voice_channel(
                                            ctx, guild_id, user_id, // Pass the full serenity context
                                        ) {
                                            Ok(id) => id,
                                            Err(err) => {
                                                let reply = embedded_messages::user_not_in_voice_channel(err);
                                                if let Err(e) = modal_interaction
                                                    // Use builder pattern for response message
                                                    .create_response(&ctx.http, CreateInteractionResponse::Message(
                                                        CreateInteractionResponseMessage::new().add_embed(reply.embeds.remove(0)).ephemeral(true)
                                                    ))
                                                    .await
                                                {
                                                    error!("Failed to send modal error response: {}", e);
                                                }
                                                return Ok(()); // Return early on error
                                            }
                                        };

                                        // --- Defer and Process ---
                                        // Defer the modal response ephemerally
                                        if let Err(e) = modal_interaction.defer_ephemeral(&ctx.http).await {
                                            error!("Failed to defer modal interaction: {}", e);
                                            // Attempt to send a followup anyway, might fail
                                        }

                                        // Get songbird manager (needed for process_play_request)
                                        let manager = match songbird::serenity::get(ctx).await {
                                            Some(manager) => manager.clone(),
                                            None => {
                                                error!("Could not retrieve Songbird manager");
                                                let reply = embedded_messages::generic_error("Internal error retrieving voice client.");
                                                 if let Err(e) = modal_interaction
                                                    // Use builder pattern for followup message
                                                    .create_followup(&ctx.http, CreateInteractionResponseFollowup::new().add_embed(reply.embeds.remove(0)).ephemeral(true))
                                                    .await
                                                {
                                                    error!("Failed to send modal error followup: {}", e);
                                                }
                                                return Ok(()); // Return early
                                            }
                                        };

                                        // Call the refactored process_play_request directly
                                        match process_play_request(
                                            manager, // Pass songbird manager Arc
                                            ctx.http.clone(), // Pass http Arc
                                            // data.clone(), // Pass data Arc if needed later
                                            guild_id,
                                            voice_channel_id,
                                            &query,
                                        )
                                        .await
                                        {
                                            Ok(reply_content) => {
                                                let reply =
                                                    embedded_messages::generic_success("Music", &reply_content);
                                                if let Err(e) = modal_interaction
                                                    // Use builder pattern for followup message
                                                    .create_followup(&ctx.http, CreateInteractionResponseFollowup::new().add_embed(reply.embeds.remove(0)).ephemeral(true))
                                                    .await
                                                {
                                                    error!("Failed to send modal success followup: {}", e);
                                                }
                                            }
                                            Err(err) => {
                                                let reply = match err {
                                                    MusicError::JoinError(_) => {
                                                        embedded_messages::failed_to_join_voice_channel(err)
                                                    }
                                                    MusicError::CacheError(_) => {
                                                        embedded_messages::failed_to_process_audio_source(err)
                                                    }
                                                    MusicError::AudioSourceError(msg) => {
                                                        embedded_messages::generic_error(&msg)
                                                    }
                                                    _ => embedded_messages::generic_error(&format!(
                                                        "An unexpected error occurred: {}",
                                                        err
                                                    )),
                                                };
                                                if let Err(e) = modal_interaction
                                                     // Use builder pattern for followup message
                                                    .create_followup(&ctx.http, CreateInteractionResponseFollowup::new().add_embed(reply.embeds.remove(0)).ephemeral(true))
                                                    .await
                                                {
                                                    error!("Failed to send modal error followup: {}", e);
                                                }
                                            }
                                        }
                                    } else {
                                        warn!(
                                            "Received unhandled modal submission: {}",
                                            modal_interaction.data.custom_id
                                        );
                                    }
                                }
                                // Other interaction types (buttons, commands) are handled by poise
                                _ => {}
                            }
                        }
                        // Pass other events to the default handler or custom handlers
                        _ => {
                            // Call the original event handler logic if needed
                            // events::handle_event(ctx, event, framework, data).await?;
                        }
                    }
                    Ok(())
                })
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
