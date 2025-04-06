//! Defines the `/autoplay` command for managing the music autoplay feature.

use super::*;
use crate::commands::music::utils::{
    autoplay_manager::{is_autoplay_enabled, set_autoplay},
    embedded_messages,
    music_manager::MusicError,
};

/// Enables, disables, or toggles the music autoplay feature for the guild.
///
/// When autoplay is enabled, the bot will automatically queue related songs
/// when the current queue becomes empty. If the `enabled` argument is omitted,
/// the command toggles the current autoplay state.
#[poise::command(slash_command, category = "Music")]
pub async fn autoplay(
    ctx: Context<'_>,
    #[description = "Enable or disable autoplay"] enabled: Option<bool>,
) -> CommandResult {
    // Ensure the command is used within a guild.
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Determine the desired state: use provided argument or toggle current state.
    let new_state = match enabled {
        Some(state) => state,
        None => !is_autoplay_enabled(guild_id).await,
    };

    // Update the autoplay state using the autoplay manager.
    set_autoplay(guild_id, new_state).await;

    // Send an embed confirming the new autoplay status.
    ctx.send(embedded_messages::autoplay_status(new_state))
        .await?;

    Ok(())
}
