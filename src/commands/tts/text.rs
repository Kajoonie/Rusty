//! Cleans and validates the text given to `/say` and formats the public reply.

use super::{TtsError, TtsResult, dectalk::MAX_AUDIO_SECONDS};

/// Maximum number of characters `/say` accepts.
pub const MAX_TEXT_CHARS: usize = 1_000;

/// Maximum length of a Discord message, counted in UTF-16 code units to stay on the safe side.
const DISCORD_MESSAGE_LIMIT: usize = 2_000;

/// Longest display name Discord allows.
const MAX_NAME_CHARS: usize = 32;

/// Characters that start or end Discord formatting, mentions, links and emoji.
const MARKDOWN_SPECIALS: &[char] = &[
    '\\', '*', '_', '~', '`', '|', '[', ']', '(', ')', '<', '>', ':', '@', '#',
];

/// Prepares raw user text for DECtalk.
///
/// Line breaks and tabs become spaces so the whole text is read as one line, every
/// other control character is removed, and surrounding whitespace is trimmed.
pub fn clean_text(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect();

    cleaned.trim().to_string()
}

/// Cleans the text and checks it against the length limit.
///
/// Returns the cleaned text, or `EmptyText` / `TextTooLong`.
pub fn validate_text(raw: &str) -> TtsResult<String> {
    let cleaned = clean_text(raw);

    if cleaned.is_empty() {
        return Err(TtsError::EmptyText);
    }

    let length = cleaned.chars().count();
    if length > MAX_TEXT_CHARS {
        return Err(TtsError::TextTooLong {
            length,
            limit: MAX_TEXT_CHARS,
        });
    }

    Ok(cleaned)
}

/// Returns the backslash-escaped form of a character, so Discord shows it literally.
fn escape_char(c: char) -> String {
    if MARKDOWN_SPECIALS.contains(&c) {
        format!("\\{c}")
    } else {
        c.to_string()
    }
}

/// Escapes Discord formatting in a string.
fn escape_markdown(input: &str) -> String {
    input.chars().map(escape_char).collect()
}

/// Length of a string as Discord counts it at worst (UTF-16 code units).
fn message_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Builds the public reply showing who said what, e.g. `🗣️ **Name**: text`.
///
/// Name and text are escaped so they cannot add formatting, links or mentions, and
/// the text is shortened with `…` if the message would exceed Discord's limit.
/// When `cut_off` is set a note says the audio was capped.
pub fn format_reply(display_name: &str, text: &str, cut_off: bool) -> String {
    let name: String = display_name.chars().take(MAX_NAME_CHARS).collect();
    let mut reply = format!("🗣️ **{}**: ", escape_markdown(&name));

    let note = if cut_off {
        format!("\n-# Cut off after {MAX_AUDIO_SECONDS} seconds.")
    } else {
        String::new()
    };

    // Leave room for the note and for the ellipsis added when the text is shortened.
    let budget = DISCORD_MESSAGE_LIMIT - message_len(&note) - 1;
    let mut used = message_len(&reply);

    for c in text.chars() {
        let piece = escape_char(c);
        let piece_len = message_len(&piece);

        if used + piece_len > budget {
            reply.push('…');
            break;
        }

        reply.push_str(&piece);
        used += piece_len;
    }

    reply.push_str(&note);
    reply
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_text_keeps_dectalk_commands() {
        let text = "[:nh] hello [:dv ap 200] [:rate 300] [:t430,500] [dah<600,20>]";
        assert_eq!(clean_text(text), text);
    }

    #[test]
    fn test_clean_text_turns_line_breaks_and_tabs_into_spaces() {
        assert_eq!(clean_text("one\ntwo\r\nthree\tfour"), "one two  three four");
    }

    #[test]
    fn test_clean_text_strips_control_characters() {
        // Includes ^K, which DECtalk treats as its "force" character.
        assert_eq!(
            clean_text("a\u{0}b\u{7}c\u{b}d\u{1b}[0me\u{7f}f"),
            "abcd[0mef"
        );
        assert_eq!(clean_text("caf\u{e9} \u{85}ok"), "caf\u{e9} ok");
    }

    #[test]
    fn test_clean_text_trims() {
        assert_eq!(clean_text("  \n hello \t "), "hello");
    }

    #[test]
    fn test_validate_text_rejects_empty() {
        assert_eq!(validate_text(""), Err(TtsError::EmptyText));
        assert_eq!(validate_text(" \n\t\u{0} "), Err(TtsError::EmptyText));
    }

    #[test]
    fn test_validate_text_accepts_text_at_the_limit() {
        let text = "a".repeat(MAX_TEXT_CHARS);
        assert_eq!(validate_text(&text), Ok(text));
    }

    #[test]
    fn test_validate_text_counts_characters_not_bytes() {
        let text = "é".repeat(MAX_TEXT_CHARS);
        assert_eq!(validate_text(&text), Ok(text));
    }

    #[test]
    fn test_validate_text_rejects_text_over_the_limit() {
        let text = "a".repeat(MAX_TEXT_CHARS + 1);
        let error = validate_text(&text).unwrap_err();
        assert_eq!(
            error,
            TtsError::TextTooLong {
                length: MAX_TEXT_CHARS + 1,
                limit: MAX_TEXT_CHARS
            }
        );
        assert_eq!(
            error.to_string(),
            "That text is too long (1001 characters). The limit is 1000."
        );
    }

    #[test]
    fn test_validate_text_keeps_option_like_text() {
        // Text is sent to DECtalk on stdin, so it needs no special treatment here.
        assert_eq!(validate_text("-h"), Ok("-h".to_string()));
    }

    #[test]
    fn test_format_reply_plain() {
        assert_eq!(
            format_reply("Charles", "hello world", false),
            "🗣️ **Charles**: hello world"
        );
    }

    #[test]
    fn test_format_reply_escapes_formatting_and_mentions() {
        let reply = format_reply(
            "**x**",
            "@everyone <@123> [link](https://e.x) `c` _i_ ||s||",
            false,
        );
        assert_eq!(
            reply,
            "🗣️ **\\*\\*x\\*\\***: \\@everyone \\<\\@123\\> \\[link\\]\\(https\\://e.x\\) \\`c\\` \\_i\\_ \\|\\|s\\|\\|"
        );
    }

    #[test]
    fn test_format_reply_shows_dectalk_commands_readably() {
        assert_eq!(
            format_reply("Bob", "[:nh] hi [dah<600,20>]", false),
            "🗣️ **Bob**: \\[\\:nh\\] hi \\[dah\\<600,20\\>\\]"
        );
    }

    #[test]
    fn test_format_reply_stays_within_the_message_limit() {
        // Every character needs escaping, doubling the length.
        let text = "*".repeat(MAX_TEXT_CHARS);
        let name = "_".repeat(MAX_NAME_CHARS);

        for cut_off in [false, true] {
            let reply = format_reply(&name, &text, cut_off);
            assert!(message_len(&reply) <= DISCORD_MESSAGE_LIMIT);
            assert!(reply.contains('…'));
            // The shortened text must not end in a lone backslash that would escape the ellipsis.
            assert!(reply.contains("\\*…"));
        }
    }

    #[test]
    fn test_format_reply_counts_wide_characters() {
        // Each emoji is two UTF-16 code units.
        let text = "😀".repeat(MAX_TEXT_CHARS);
        let reply = format_reply("Bob", &text, true);
        assert!(message_len(&reply) <= DISCORD_MESSAGE_LIMIT);
        assert!(reply.ends_with("\n-# Cut off after 60 seconds."));
    }

    #[test]
    fn test_format_reply_does_not_shorten_text_at_the_limit() {
        let text = "a".repeat(MAX_TEXT_CHARS);
        let reply = format_reply("Bob", &text, false);
        assert!(reply.ends_with(&text));
        assert!(!reply.contains('…'));
    }

    #[test]
    fn test_format_reply_notes_cut_off_audio() {
        assert_eq!(
            format_reply("Bob", "hi", true),
            "🗣️ **Bob**: hi\n-# Cut off after 60 seconds."
        );
    }

    #[test]
    fn test_format_reply_limits_name_length() {
        let reply = format_reply(&"n".repeat(500), "hi", false);
        assert_eq!(reply, format!("🗣️ **{}**: hi", "n".repeat(MAX_NAME_CHARS)));
    }
}
