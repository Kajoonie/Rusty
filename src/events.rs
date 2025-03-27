use std::error::Error;

use crate::{Data, commands::music::utils::button_handlers::handle_button_interaction};
use poise::serenity_prelude::{Context as SerenityContext, FullEvent, Interaction};
use tracing::error;

pub async fn handle_event(
    ctx: &SerenityContext,
    event: &FullEvent,
    _framework: poise::FrameworkContext<'_, Data, Box<dyn Error + Send + Sync>>,
    _user_data: &Data,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let FullEvent::InteractionCreate { interaction } = event {
        if let Interaction::Component(mut component) = interaction.clone() {
            if let Err(e) = handle_button_interaction(ctx, &mut component).await {
                error!("Error handling button interaction: {}", e);
            }
        }
    }
    Ok(())
}
