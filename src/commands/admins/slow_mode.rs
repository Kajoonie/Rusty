use crate::{CommandResult, Context, is_admin};
use poise::serenity_prelude as serenity;

#[poise::command(
    slash_command,
    check = "is_admin",
    category = "Admin",
)]
pub async fn slow_mode(
    ctx: Context<'_>,
    #[description = "Minimum time between sending messages per user"] rate_limit: Option<u64>,
) -> CommandResult {
    let say_content = if let Some(rate_limit) = rate_limit {
        if let Err(why) = ctx
            .channel_id()
            .edit(ctx, |c| c.rate_limit_per_user(rate_limit))
            .await
        {
            println!("Error setting channel's slow mode rate {why:?}");
            format!("Failed to set the slow mode to `{rate_limit}` seconds.")
        } else {
            format!("Successfully set slow mode rate to `{rate_limit}` seconds.")
        }
    } else if let Some(serenity::Channel::Guild(channel)) = ctx.channel_id().to_channel_cached(ctx)
    {
        format!(
            "Current slow mode rate is `{}` seconds.",
            channel.rate_limit_per_user.unwrap_or(0)
        )
    } else {
        "Failed to find channel in cache.".into()
    };

    ctx.say(say_content).await?;

    Ok(())
}
