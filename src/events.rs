use std::env;
use std::time::Duration;

use serenity::all::{ComponentInteraction, InputTextStyle, ModalInteraction};
use serenity::async_trait;
use serenity::builder::{
    CreateActionRow, CreateButton, CreateCommand, CreateInputText, CreateInteractionResponse,
    CreateInteractionResponseMessage,
};
use serenity::model::{application::Interaction, gateway::Ready, id::GuildId};
use serenity::prelude::*;
use serenity::utils::CreateQuickModal;
use tracing::{debug, error};

use crate::commands::music::utils::component_handlers;

pub struct Handler;

#[async_trait]
impl serenity::prelude::EventHandler for Handler {
    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        match interaction {
            Interaction::Component(mut component) => {
                if component.data.custom_id.starts_with("music_") {
                    music_component_interaction(&ctx, &mut component).await;
                }
            }
            Interaction::Modal(modal) => {
                debug!("Modal interaction: {:?}", modal)
            }
            _ => (),
        }
    }
}

/// Handle component interactions for components with identities starting with "music_"
async fn music_component_interaction(ctx: &Context, mut component: &mut ComponentInteraction) {
    if let Err(e) = component_handlers::handle_interaction(&ctx, &mut component).await {
        error!("Error handling component interaction: {}", e);
    }
}

/// Handle modal interactions for modals with identities starting with "music_"
async fn music_modal_interaction(ctx: &Context, modal: ModalInteraction) {
    todo!()
}
