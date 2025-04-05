use super::*;
use crate::commands::music::{
    audio_sources::track_metadata::TrackMetadata,
    utils::{
        embedded_messages,
        music_manager::{MusicError, MusicManager},
    },
};

/// Remove a track from the queue by its position
#[poise::command(slash_command, category = "Music")]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Position of the track to remove"]
    #[min = 1]
    position: usize,
) -> CommandResult {
    ctx.defer_ephemeral().await?;

    let guild_id = ctx.guild_id().ok_or_else(|| {
        Box::new(MusicError::NotInGuild) as Box<dyn std::error::Error + Send + Sync>
    })?;

    let queue = MusicManager::get_queue(&guild_id).await;

    if let Some(queue) = queue {
        match queue.dequeue(position) {
            Some(queued) => {
                let data = queued.data::<TrackMetadata>();
                ctx.send(embedded_messages::track_removed(&data, position))
                    .await?;
            }
            None => {
                ctx.send(embedded_messages::invalid_queue_position(queue.len()))
                    .await?;
            }
        }
    } else {
        ctx.send(embedded_messages::queue_is_empty()).await?;
    }

    Ok(())
}
