//! Defines the `/say` command for speaking text in a voice channel with DECtalk.

use super::playback::QueuedClip;
use super::*;
use crate::commands::music::utils::embedded_messages;
use poise::{CreateReply, serenity_prelude as serenity};
use tracing::{debug, info};

/// Speaks text in your voice channel with the DECtalk voice from Moonbase Alpha.
///
/// DECtalk's inline commands work: voices like `[:nh]` and `[:np]`, `[:dv ap 200]`,
/// `[:rate 300]`, tones like `[:t430,500]` and sung phonemes like `[dah<600,20>]`.
/// The bot joins your voice channel if it is not connected yet, and speaks over any
/// music that is playing. Requests are spoken one after another.
#[poise::command(slash_command, prefix_command, guild_only, category = "TTS")]
pub async fn say(
    ctx: Context<'_>,
    #[description = "Text to speak; DECtalk commands such as [:nh] or [dah<600,20>] work"]
    #[rest]
    text: String,
) -> CommandResult {
    // Ensure the command is used within a guild.
    let guild_id = ctx.guild_id().ok_or(TtsError::NotInGuild)?;
    let user_id = ctx.author().id;
    let config = dectalk::config();

    // Checks that need no waiting come first, so their errors can be replied to
    // ephemerally before a public response is started.
    let text = match early_checks(ctx, guild_id, user_id, config, &text).await {
        Ok(text) => text,
        Err(e) => {
            ctx.send(embedded_messages::generic_error(&e.to_string()))
                .await?;
            return Ok(());
        }
    };

    // The text itself is not logged.
    info!(
        "Received /say command with {} characters in guild {}",
        text.chars().count(),
        guild_id
    );

    // Take a place in line right away, while the public response is being deferred
    // (synthesis and joining a channel can take longer than Discord's reply window).
    let queue = playback::guild_queue(guild_id);
    let (deferred, turn) = tokio::join!(ctx.defer(), queue.wait_turn());
    deferred?;

    // Synthesize, connect and queue while holding this guild's turn, so clips are
    // queued in the order the requests arrived.
    let queued = queue_speech(ctx, &queue, guild_id, user_id, config, &text).await;
    drop(turn);

    match queued {
        Ok(cut_off) => {
            // Show who said what. Mentions are disabled so the text cannot ping anyone.
            let name = display_name(ctx).await;
            let reply = CreateReply::default()
                .content(text::format_reply(&name, &text, cut_off))
                .allowed_mentions(serenity::CreateAllowedMentions::new());
            ctx.send(reply).await?;
        }
        Err(e) => report_late_error(ctx, &e.to_string()).await?,
    }

    Ok(())
}

/// Synthesizes `text`, joins the caller's voice channel if needed and queues the clip.
///
/// Call this while holding the guild's turn (`GuildQueue::wait_turn`), so clips are
/// queued in the order their requests arrived. Returns whether the audio was cut off.
pub(super) async fn queue_speech(
    ctx: Context<'_>,
    queue: &playback::GuildQueue,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
    config: &dectalk::DectalkConfig,
    text: &str,
) -> TtsResult<bool> {
    let slot = queue.reserve()?;
    // The channels may have changed while waiting for the turn.
    playback::check_voice(ctx.serenity_context(), guild_id, user_id).await?;
    let clip = dectalk::synthesize(config, text).await?;
    let call = playback::connect(ctx.serenity_context(), guild_id, user_id).await?;

    let cut_off = clip.cut_off;
    slot.fill(QueuedClip { call, clip });

    Ok(cut_off)
}

/// Reports an error that happened after the public response was deferred.
///
/// The deferred response is public and cannot be turned into an ephemeral one, so it is
/// removed and the error is sent as a follow-up instead.
pub(super) async fn report_late_error(ctx: Context<'_>, message: &str) -> CommandResult {
    if let Context::Application(app_ctx) = ctx {
        let deleted = app_ctx.interaction.delete_response(ctx.http()).await;
        if let Err(delete_error) = deleted {
            debug!(
                "Failed to delete deferred /{} response: {}",
                ctx.command().name,
                delete_error
            );
        }
    }
    ctx.send(embedded_messages::generic_error(message)).await?;

    Ok(())
}

/// Runs the checks that are answered instantly: the text, DECtalk being installed,
/// and the voice channels. Returns the cleaned text.
async fn early_checks(
    ctx: Context<'_>,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
    config: &dectalk::DectalkConfig,
    raw_text: &str,
) -> TtsResult<String> {
    let text = text::validate_text(raw_text)?;

    voice_checks(ctx, guild_id, user_id, config).await?;

    Ok(text)
}

/// Checks that DECtalk is installed, the caller is in a voice channel and the bot is
/// not busy in another one.
pub(super) async fn voice_checks(
    ctx: Context<'_>,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
    config: &dectalk::DectalkConfig,
) -> TtsResult<()> {
    if !config.is_installed() {
        return Err(TtsError::NotInstalled);
    }

    playback::check_voice(ctx.serenity_context(), guild_id, user_id).await?;

    Ok(())
}

/// The caller's name as shown in the guild: nickname, then global name, then username.
pub(super) async fn display_name(ctx: Context<'_>) -> String {
    match ctx.author_member().await {
        Some(member) => member.display_name().to_string(),
        None => ctx.author().display_name().to_string(),
    }
}
