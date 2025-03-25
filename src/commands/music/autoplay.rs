use super::*;
use crate::commands::music::utils::{
    autoplay_manager::{is_autoplay_enabled, set_autoplay},
    music_manager::MusicError,
};

/// Toggle autoplay feature (automatically play related songs when queue is empty)
#[poise::command(slash_command, category = "Music")]
pub async fn autoplay(
    ctx: Context<'_>,
    #[description = "Enable or disable autoplay"] enabled: Option<bool>,
) -> CommandResult {
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // If no argument is provided, toggle the current state
    let new_state = match enabled {
        Some(state) => state,
        None => !is_autoplay_enabled(guild_id).await,
    };

    // Update the autoplay state
    set_autoplay(guild_id, new_state).await;

    // Send confirmation message
    ctx.send(
        CreateReply::default().embed(
            CreateEmbed::new()
                .title(if new_state {
                    "🔄 Autoplay Enabled"
                } else {
                    "⏹️ Autoplay Disabled"
                })
                .description(if new_state {
                    "I will automatically play related songs when the queue is empty"
                } else {
                    "I will stop playing when the queue is empty"
                })
                .color(if new_state { 0x00ff00 } else { 0xff0000 }),
        ),
    )
    .await?;

    Ok(())
}
