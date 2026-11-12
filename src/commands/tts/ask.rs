//! Defines the `/ask` command: Claude answers a question and DECtalk speaks the answer.

use super::*;
use crate::commands::music::utils::embedded_messages;
use crate::utils::anthropic_client::{self, AnthropicClient, ClaudeError};
use poise::{CreateReply, serenity_prelude as serenity};
use tracing::info;

/// Maximum number of characters `/ask` accepts as a question.
///
/// Shorter than `/say`'s limit: a question is a sentence or two, it is repeated in the
/// reply next to the answer, and every character of it is paid for.
pub const MAX_QUESTION_CHARS: usize = 500;

const _: () = assert!(MAX_QUESTION_CHARS <= text::MAX_TEXT_CHARS);

/// Longest display name Discord allows; longer input is cut before it is sent to Claude.
const MAX_NAME_CHARS: usize = 32;

/// Represents the ways `/ask` can fail. The messages are shown to Discord users.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum AskError {
    /// The question was empty after cleaning.
    #[error("There is no question. Ask me something.")]
    EmptyQuestion,

    /// The question exceeds `MAX_QUESTION_CHARS`.
    #[error("That question is too long ({length} characters). The limit is {limit}.")]
    QuestionTooLong {
        /// Number of characters in the cleaned question.
        length: usize,
        /// The maximum number of characters allowed.
        limit: usize,
    },

    /// Claude could not be asked or gave no usable answer.
    #[error(transparent)]
    Claude(#[from] ClaudeError),

    /// The answer could not be spoken.
    #[error(transparent)]
    Tts(#[from] TtsError),
}

/// Asks Rusty a question; Claude answers and DECtalk speaks it in your voice channel.
///
/// Rusty answers out loud in the DECtalk voice from Moonbase Alpha, for everyone in the
/// voice channel to hear; the question and the answer as text are visible only to you.
/// Ask for a song, a different voice or a sound effect and Rusty will try that too. Each
/// question stands alone; Rusty does not remember earlier ones. The bot joins your voice
/// channel if it is not connected yet, and answers are spoken one after another together
/// with `/say` messages.
#[poise::command(slash_command, prefix_command, guild_only, category = "TTS")]
pub async fn ask(
    ctx: Context<'_>,
    #[description = "Your question for Rusty; you can also ask for a song or a voice"]
    #[rest]
    question: String,
) -> CommandResult {
    // Ensure the command is used within a guild.
    let guild_id = ctx.guild_id().ok_or(TtsError::NotInGuild)?;
    let user_id = ctx.author().id;
    let config = dectalk::config();
    let claude = anthropic_client::client();

    // Checks that need no waiting come first, so their errors are replied to at once
    // and before Claude is asked.
    let question = match early_checks(ctx, guild_id, user_id, config, claude, &question).await {
        Ok(question) => question,
        Err(e) => {
            ctx.send(embedded_messages::generic_error(&e.to_string()))
                .await?;
            return Ok(());
        }
    };

    // The question itself is not logged.
    info!(
        "Received /ask command with {} characters in guild {}",
        question.chars().count(),
        guild_id
    );

    // Claude takes longer than Discord's reply window. The defer is ephemeral, so only
    // the asker sees the reply; the prefix form has no ephemeral messages and replies in
    // the channel.
    ctx.defer_ephemeral().await?;

    let asker = speak::display_name(ctx).await;

    match answer_and_speak(ctx, guild_id, user_id, config, claude, &asker, &question).await {
        Ok((answer, cut_off)) => {
            // Show the asker both sides. Mentions are disabled so the text cannot ping
            // anyone where the reply is public (the prefix form).
            let reply = CreateReply::default()
                .content(text::format_ask_reply(&asker, &question, &answer, cut_off))
                .allowed_mentions(serenity::CreateAllowedMentions::new())
                .ephemeral(true);
            ctx.send(reply).await?;
        }
        Err(e) => {
            ctx.send(embedded_messages::generic_error(&e.to_string()))
                .await?;
        }
    }

    Ok(())
}

/// Runs the checks that are answered instantly: the question, the API key, DECtalk
/// being installed, and the voice channels. Returns the cleaned question.
async fn early_checks(
    ctx: Context<'_>,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
    config: &dectalk::DectalkConfig,
    claude: &AnthropicClient,
    raw_question: &str,
) -> Result<String, AskError> {
    let question = validate_question(raw_question)?;

    if !claude.has_api_key() {
        return Err(ClaudeError::MissingKey.into());
    }

    speak::voice_checks(ctx, guild_id, user_id, config).await?;

    Ok(question)
}

/// Gets Claude's answer, then speaks it the way `/say` speaks text.
///
/// Returns the answer as it is spoken and whether its audio was cut off.
async fn answer_and_speak(
    ctx: Context<'_>,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
    config: &dectalk::DectalkConfig,
    claude: &AnthropicClient,
    asker: &str,
    question: &str,
) -> Result<(String, bool), AskError> {
    // The guild's turn is not held while waiting for Claude, which can take a minute:
    // `/say` and other answers keep flowing meanwhile. The turn is taken only once there
    // is something to synthesize, so answers are spoken in the order they arrive.
    let raw_answer = claude
        .ask(prompt::system_prompt(), &user_message(asker, question))
        .await?;

    let answer = answer::prepare_answer(&raw_answer);
    if answer.is_empty() {
        return Err(ClaudeError::EmptyAnswer.into());
    }
    info!(
        "Claude answered /ask in guild {} with {} characters",
        guild_id,
        answer.chars().count()
    );

    let queue = playback::guild_queue(guild_id);
    let turn = queue.wait_turn().await;
    let spoken = answer::spoken_text(&answer);
    let queued = speak::queue_speech(ctx, &queue, guild_id, user_id, config, &spoken).await;
    drop(turn);

    Ok((answer, queued?))
}

/// Cleans the question and checks it against the length limit.
fn validate_question(raw: &str) -> Result<String, AskError> {
    let cleaned = text::clean_text(raw);

    if cleaned.is_empty() {
        return Err(AskError::EmptyQuestion);
    }

    let length = cleaned.chars().count();
    if length > MAX_QUESTION_CHARS {
        return Err(AskError::QuestionTooLong {
            length,
            limit: MAX_QUESTION_CHARS,
        });
    }

    Ok(cleaned)
}

/// Builds the user message for Claude: the question, with the asker's name in front.
fn user_message(asker: &str, question: &str) -> String {
    let name: String = text::clean_text(asker)
        .chars()
        .take(MAX_NAME_CHARS)
        .collect();
    let name = name.trim();

    if name.is_empty() {
        question.to_string()
    } else {
        format!("{name} asks: {question}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_question_cleans_like_say() {
        assert_eq!(
            validate_question("  why is\nthe sky\tblue?\u{0} "),
            Ok("why is the sky blue?".to_string())
        );
    }

    #[test]
    fn test_validate_question_rejects_empty() {
        assert_eq!(validate_question(""), Err(AskError::EmptyQuestion));
        assert_eq!(validate_question(" \n\t "), Err(AskError::EmptyQuestion));
    }

    #[test]
    fn test_validate_question_enforces_the_limit() {
        let at_limit = "é".repeat(MAX_QUESTION_CHARS);
        assert_eq!(validate_question(&at_limit), Ok(at_limit.clone()));

        let error = validate_question(&(at_limit + "!")).unwrap_err();
        assert_eq!(
            error,
            AskError::QuestionTooLong {
                length: MAX_QUESTION_CHARS + 1,
                limit: MAX_QUESTION_CHARS
            }
        );
        assert_eq!(
            error.to_string(),
            format!(
                "That question is too long ({} characters). The limit is {}.",
                MAX_QUESTION_CHARS + 1,
                MAX_QUESTION_CHARS
            )
        );
    }

    #[test]
    fn test_user_message_names_the_asker() {
        assert_eq!(
            user_message("Charles", "why is the sky blue?"),
            "Charles asks: why is the sky blue?"
        );
    }

    #[test]
    fn test_user_message_cleans_and_limits_the_name() {
        assert_eq!(user_message(" \n ", "q"), "q");
        assert_eq!(user_message("a\nb\u{0}", "q"), "a b asks: q");
        assert_eq!(
            user_message(&"n".repeat(100), "q"),
            format!("{} asks: q", "n".repeat(MAX_NAME_CHARS))
        );
    }

    #[test]
    fn test_errors_pass_through_their_messages() {
        let missing_key: AskError = ClaudeError::MissingKey.into();
        assert_eq!(missing_key.to_string(), ClaudeError::MissingKey.to_string());
        assert!(missing_key.to_string().contains("ANTHROPIC_API_KEY"));

        let not_in_voice: AskError = TtsError::UserNotInVoiceChannel.into();
        assert_eq!(
            not_in_voice.to_string(),
            TtsError::UserNotInVoiceChannel.to_string()
        );
    }

    #[test]
    fn test_a_question_and_a_long_spoken_answer_fit_one_message() {
        // The longest question with an answer of typical spoken length is shown whole.
        let question = "q".repeat(MAX_QUESTION_CHARS);
        let answer = "a".repeat(700);
        let reply = text::format_ask_reply("Bob", &question, &answer, true);
        assert!(reply.contains(&question));
        assert!(reply.contains(&answer));
        assert!(!reply.contains('…'));
    }
}
