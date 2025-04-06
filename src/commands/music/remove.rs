//! Defines the `/remove` command for removing tracks from the music queue.

use super::*;
use crate::commands::music::{
    audio_sources::track_metadata::TrackMetadata,
    utils::{
        embedded_messages,
        music_manager::{MusicError, MusicManager},
    },
};

/// Removes a track from the music queue based on its 1-based position.
///
/// Takes the position number as input. If the position is valid and the queue
/// is not empty, the track is removed, and a confirmation message is sent.
/// Otherwise, an appropriate error message is displayed.
#[poise::command(slash_command, category = "Music")]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Position of the track to remove"]
    #[min = 1]
    position: usize,
) -> CommandResult {
    // Defer response ephemerally.
    ctx.defer_ephemeral().await?;

    // Ensure the command is used within a guild.
    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    // Get the current queue for the guild.
    let queue = MusicManager::get_queue(&guild_id).await;

    if let Some(queue) = queue {
        // Attempt to remove the track at the specified position (1-based index).
        match queue.dequeue(position) {
            Some(queued) => {
                // Get metadata of the removed track.
                let data = queued.data::<TrackMetadata>();
                // Send confirmation message.
                ctx.send(embedded_messages::track_removed(&data, position))
                    .await?;
            }
            None => {
                // Send error message for invalid position.
                ctx.send(embedded_messages::invalid_queue_position(queue.len()))
                    .await?;
            }
        }
    } else {
        // Send message if the queue is empty.
        ctx.send(embedded_messages::queue_is_empty()).await?;
    }

    Ok(())
}
