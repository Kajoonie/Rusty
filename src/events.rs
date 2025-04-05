use serenity::all::ComponentInteraction;
use serenity::async_trait;
use serenity::model::application::Interaction;
use serenity::prelude::*;
use tracing::error;

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
            // Interaction::Modal(mut modal) => {
            //     if modal.data.custom_id.starts_with("music_") {
            //         music_modal_interaction(&ctx, &mut modal).await;
            //     }
            // }
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

// /// Handle modal interactions for modals with identities starting with "music_"
// async fn music_modal_interaction(ctx: &Context, mut modal: &mut ModalInteraction) {
//     if let Err(e) = modal_handlers::handle_interaction(&ctx, &mut modal).await {
//         error!("Error handling modal interaction: {}", e);
//     }
// }
