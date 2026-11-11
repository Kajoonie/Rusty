//! This module handles Discord gateway events, specifically interaction events.

#[cfg(feature = "music")]
use serenity::all::ComponentInteraction;
use serenity::async_trait;
use serenity::model::application::Interaction;
use serenity::prelude::*;
#[cfg(feature = "music")]
use tracing::error;

#[cfg(feature = "music")]
use crate::commands::music::utils::component_handlers;

/// The main event handler struct for the bot.
///
/// Implements the `serenity::prelude::EventHandler` trait to receive and process events.
pub struct Handler;

#[async_trait]
impl serenity::prelude::EventHandler for Handler {
    /// Called when a new interaction is created (e.g., slash command, button press).
    ///
    /// Currently, it only handles component interactions (like buttons) whose custom IDs
    /// start with "music_", delegating them to `music_component_interaction`.
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        #[cfg(feature = "music")]
        if let Interaction::Component(mut component) = interaction {
            if component.data.custom_id.starts_with("music_") {
                music_component_interaction(&ctx, &mut component).await;
            }
        }

        // Without the music feature there are no components to handle.
        #[cfg(not(feature = "music"))]
        let _ = (ctx, interaction);
    }
}

/// Handle component interactions for components with identities starting with "music_"
#[cfg(feature = "music")]
async fn music_component_interaction(ctx: &Context, component: &mut ComponentInteraction) {
    if let Err(e) = component_handlers::handle_interaction(ctx, component).await {
        error!("Error handling component interaction: {}", e);
    }
}
