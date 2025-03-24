use super::*;

/// Get the current AI model being used by the user.
#[poise::command(slash_command, category = "AI")]
pub async fn get_model(ctx: Context<'_>) -> CommandResult {
    ctx.defer().await?;

    let author = ctx.author();
    let model = database::get_user_model(&author);

    ctx.say(format!("Your currently active model is: **{}**", model)).await?;
    Ok(())
}