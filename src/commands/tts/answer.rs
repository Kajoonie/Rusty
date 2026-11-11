//! Prepares Claude's answer to `/ask` for DECtalk.
//!
//! The answer is written by a model, not typed by a person, so it is made safe to speak
//! before it reaches the synthesizer: plain ASCII (this DECtalk build produces no audio at
//! all for text with any other character), balanced square brackets (an unclosed one makes
//! DECtalk read the rest of the text as phonemes), and a length within the limit, cut
//! where it does not break a sentence or a DECtalk command.

use super::text::{self, MAX_TEXT_CHARS};

/// Put in front of every answer when it is spoken (not when it is shown).
///
/// By default DECtalk reads an error message aloud when it meets a command or phoneme it
/// does not know. With this, it skips the mistake silently instead, so one wrong note in
/// a song costs that note rather than two seconds of error message.
pub const SPOKEN_PREFIX: &str = "[:error ignore]";

/// Characters kept free below `MAX_TEXT_CHARS`, for `SPOKEN_PREFIX` and some slack.
const ANSWER_MARGIN: usize = 50;

/// Maximum number of characters of an answer. A song needs the room; spoken answers are
/// kept far shorter by the system prompt, which states this limit.
pub const MAX_ANSWER_CHARS: usize = MAX_TEXT_CHARS - ANSWER_MARGIN;

const _: () = assert!(SPOKEN_PREFIX.len() <= ANSWER_MARGIN);

/// Makes a raw answer speakable: ASCII only, one line, balanced brackets, at most
/// `MAX_ANSWER_CHARS` characters. Returns an empty string if nothing speakable is left.
pub fn prepare_answer(raw: &str) -> String {
    let ascii = to_ascii(raw);
    let balanced = balance_brackets(&text::clean_text(&ascii));
    let single_spaced = balanced.split_whitespace().collect::<Vec<_>>().join(" ");

    shorten_speech(&single_spaced, MAX_ANSWER_CHARS)
}

/// The text handed to DECtalk for a prepared answer.
pub fn spoken_text(answer: &str) -> String {
    format!("{SPOKEN_PREFIX}{answer}")
}

/// The ASCII replacement for a character DECtalk cannot take, or nothing if there is none.
fn ascii_replacement(c: char) -> &'static str {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{2032}' | '\u{00B4}' => "'",
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{2033}' | '\u{00AB}' | '\u{00BB}' => "\"",
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2212}' => "-",
        // A dash between clauses is a pause.
        '\u{2014}' | '\u{2015}' => ",",
        '\u{2026}' => "...",
        '\u{00A0}' | '\u{2000}'..='\u{200A}' | '\u{202F}' | '\u{3000}' => " ",
        '\u{00D7}' => "x",
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a",
        'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' => "A",
        'ç' | 'ć' | 'č' => "c",
        'Ç' | 'Ć' | 'Č' => "C",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' => "e",
        'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ę' => "E",
        'ì' | 'í' | 'î' | 'ï' | 'ī' => "i",
        'Ì' | 'Í' | 'Î' | 'Ï' | 'Ī' => "I",
        'ñ' | 'ń' => "n",
        'Ñ' | 'Ń' => "N",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => "o",
        'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ō' => "O",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' => "u",
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ū' => "U",
        'ý' | 'ÿ' => "y",
        'Ý' => "Y",
        'ś' | 'š' => "s",
        'Ś' | 'Š' => "S",
        'ź' | 'ż' | 'ž' => "z",
        'Ź' | 'Ż' | 'Ž' => "Z",
        'ł' => "l",
        'Ł' => "L",
        'ß' => "ss",
        'æ' => "ae",
        'Æ' => "AE",
        'œ' => "oe",
        'Œ' => "OE",
        _ => "",
    }
}

/// Reduces text to ASCII: typographic punctuation and accented letters become their
/// plain forms, and anything else that is not ASCII (emoji, other scripts) is removed.
/// Backticks are removed too; they only ever come from markdown.
pub fn to_ascii(input: &str) -> String {
    let mut output = String::with_capacity(input.len());

    for c in input.chars() {
        match c {
            '`' => {}
            c if c.is_ascii() => output.push(c),
            c => output.push_str(ascii_replacement(c)),
        }
    }

    output
}

/// Makes the square brackets of a text balanced and flat, as DECtalk needs them.
///
/// A `]` with no open bracket and a `[` inside an open bracket are removed, and a
/// bracket left open at the end is closed.
pub fn balance_brackets(input: &str) -> String {
    let mut output = String::with_capacity(input.len() + 1);
    let mut inside = false;

    for c in input.chars() {
        match c {
            '[' if inside => {}
            ']' if !inside => {}
            '[' => {
                inside = true;
                output.push(c);
            }
            ']' => {
                inside = false;
                output.push(c);
            }
            c => output.push(c),
        }
    }

    if inside {
        output.push(']');
    }

    output
}

/// A place where a text can be cut.
#[derive(Clone, Copy)]
struct Cut {
    /// Byte offset the kept text ends at.
    at: usize,
    /// Number of characters kept.
    chars: usize,
}

/// Shortens a text with balanced brackets to at most `max_chars` characters without
/// breaking it. Text that fits is returned unchanged.
///
/// The cut is made, in order of preference:
/// 1. after a sentence or a bracket group (or just before one), if that keeps at least
///    half of the allowed length;
/// 2. otherwise at the latest possible place: such a boundary, a space between words, or
///    after a complete note (`...>`) of the bracket group that crosses the limit, which
///    is then closed with `]`.
///
/// A cut is never made anywhere else inside a bracket group, so the result always has
/// balanced brackets and whole commands. It is empty if there is no place to cut.
pub fn shorten_speech(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let mut inside = false;
    // The end of a sentence or bracket group, or the start of a bracket group.
    let mut boundary: Option<Cut> = None;
    // A space between words, outside brackets.
    let mut space: Option<Cut> = None;
    // The end of a note in the bracket group that is open, if one is.
    let mut note: Option<Cut> = None;

    let mut chars = text.char_indices().peekable();
    let mut kept = 0;

    while kept < max_chars {
        let Some((at, c)) = chars.next() else { break };
        let before = Cut { at, chars: kept };
        kept += 1;
        let after = Cut {
            at: at + c.len_utf8(),
            chars: kept,
        };
        let next = chars.peek().map(|&(_, next)| next);

        match c {
            '[' if !inside => {
                inside = true;
                note = None;
                if before.chars > 0 {
                    boundary = Some(before);
                }
            }
            ']' if inside => {
                inside = false;
                note = None;
                boundary = Some(after);
            }
            // Cutting after a note needs room for the closing bracket.
            '>' if inside && after.chars < max_chars => note = Some(after),
            '.' | '!' | '?' if !inside && next.is_none_or(char::is_whitespace) => {
                boundary = Some(after);
            }
            c if c.is_whitespace() && !inside => space = Some(before),
            _ => {}
        }
    }

    // The first character past the limit can still mark a place to cut right at it.
    if let (false, Some(&(at, next))) = (inside, chars.peek()) {
        let here = Cut { at, chars: kept };
        if next == '[' {
            boundary = Some(here);
        } else if next.is_whitespace() {
            space = Some(here);
        }
    }

    let preferred = boundary.filter(|cut| cut.chars * 2 >= max_chars);
    let latest = [boundary, space, note]
        .into_iter()
        .flatten()
        .max_by_key(|cut| cut.at);

    let Some(cut) = preferred.or(latest) else {
        return String::new();
    };

    let mut shortened = text[..cut.at].trim_end().to_string();
    // Only a cut after a note leaves its bracket group open.
    if note.is_some_and(|note| note.at == cut.at) && inside {
        shortened.push(']');
    }

    shortened
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether every `[` is closed by a `]` before the next `[`, and no `]` is stray.
    fn is_balanced(text: &str) -> bool {
        let mut inside = false;
        for c in text.chars() {
            match c {
                '[' if inside => return false,
                ']' if !inside => return false,
                '[' => inside = true,
                ']' => inside = false,
                _ => {}
            }
        }
        !inside
    }

    #[test]
    fn test_answer_limit_leaves_room_below_the_text_limit() {
        let longest = "a".repeat(MAX_ANSWER_CHARS);
        assert!(text::validate_text(&spoken_text(&longest)).is_ok());
    }

    #[test]
    fn test_to_ascii_keeps_ascii_and_dectalk_commands() {
        let text = "[:nh] It's \"fine\" - 3.5 [:dv ap 200] [dah<600,20>_<250>] ok?";
        assert_eq!(to_ascii(text), text);
    }

    #[test]
    fn test_to_ascii_replaces_typographic_punctuation() {
        assert_eq!(
            to_ascii("It\u{2019}s \u{201C}fine\u{201D} \u{2014} 5\u{2013}10\u{2026}"),
            "It's \"fine\" , 5-10..."
        );
        assert_eq!(to_ascii("a\u{a0}b\u{202f}c"), "a b c");
    }

    #[test]
    fn test_to_ascii_strips_accents_and_drops_the_rest() {
        assert_eq!(
            to_ascii("café Zoë naïve Ærø straße"),
            "cafe Zoe naive AEro strasse"
        );
        assert_eq!(to_ascii("ok 😀 日本 ✓ `code`"), "ok    code");
        assert!(to_ascii("Ünïcödé — “everything” … 🎵").is_ascii());
    }

    #[test]
    fn test_balance_brackets_leaves_balanced_text_alone() {
        let text = "Hi [:nh] there [dah<600,20>dah<600,22>] done [] ok";
        assert_eq!(balance_brackets(text), text);
    }

    #[test]
    fn test_balance_brackets_repairs_unbalanced_text() {
        assert_eq!(balance_brackets("a ] b"), "a  b");
        assert_eq!(balance_brackets("a [dah<600,20>"), "a [dah<600,20>]");
        assert_eq!(
            balance_brackets("[dah<600,20>[dah<600,20>]]"),
            "[dah<600,20>dah<600,20>]"
        );
        assert_eq!(balance_brackets("]]["), "[]");

        for text in ["[[a]b]c]", "[", "]", "a[b[c", "[:nh hello [dah<1,2>]"] {
            assert!(is_balanced(&balance_brackets(text)), "{text}");
        }
    }

    #[test]
    fn test_shorten_speech_keeps_text_that_fits() {
        assert_eq!(shorten_speech("Short answer.", 13), "Short answer.");
        assert_eq!(shorten_speech("", 10), "");
    }

    #[test]
    fn test_shorten_speech_cuts_at_a_sentence() {
        let text = "First sentence here. Second sentence here. Third one is cut.";
        assert_eq!(
            shorten_speech(text, 50),
            "First sentence here. Second sentence here."
        );
        // A sentence that ends exactly at the limit is kept.
        assert_eq!(
            shorten_speech(text, 42),
            "First sentence here. Second sentence here."
        );
        assert_eq!(shorten_speech(text, 30), "First sentence here.");
    }

    #[test]
    fn test_shorten_speech_ignores_periods_inside_numbers() {
        let text = "Pi is about 3.14159 and e is about 2.71828, roughly speaking, ok.";
        assert_eq!(shorten_speech(text, 30), "Pi is about 3.14159 and e is");
    }

    #[test]
    fn test_shorten_speech_falls_back_to_a_word_boundary() {
        // The only sentence end is too early to be worth cutting at.
        let text = "Hi. This goes on and on without any further sentence end at all";
        assert_eq!(shorten_speech(text, 30), "Hi. This goes on and on");
        // A word that ends exactly at the limit is kept whole.
        assert_eq!(shorten_speech("aaaa bbbb cccc", 9), "aaaa bbbb");
        // One long word cannot be cut anywhere.
        assert_eq!(shorten_speech(&"a".repeat(50), 20), "");
    }

    #[test]
    fn test_shorten_speech_cuts_songs_between_bracket_groups() {
        let song = "[dah<600,20>]".repeat(10);
        // 13 characters per group: 40 characters hold three whole groups.
        assert_eq!(shorten_speech(&song, 40), "[dah<600,20>]".repeat(3));
        assert_eq!(shorten_speech(&song, 39), "[dah<600,20>]".repeat(3));
        assert_eq!(shorten_speech(&song, 38), "[dah<600,20>]".repeat(2));
    }

    #[test]
    fn test_shorten_speech_never_cuts_inside_a_command() {
        let text = "This sentence is here to fill up space [:dv ap 200 hs 80] hello";
        // The limit falls inside the command; the cut is made before it.
        for limit in 40..57 {
            assert_eq!(
                shorten_speech(text, limit),
                "This sentence is here to fill up space",
                "limit {limit}"
            );
        }
        assert_eq!(
            shorten_speech(text, 57),
            "This sentence is here to fill up space [:dv ap 200 hs 80]"
        );
    }

    #[test]
    fn test_shorten_speech_closes_a_long_group_after_a_whole_note() {
        // One bracket group far longer than the limit.
        let song = format!("[{}]", "dah<600,20>".repeat(20));
        assert_eq!(shorten_speech(&song, 30), "[dah<600,20>dah<600,20>]");
        // Words before the group are kept when the group is cut.
        let intro = format!("La. [{}]", "dah<600,20>".repeat(20));
        assert_eq!(shorten_speech(&intro, 30), "La. [dah<600,20>dah<600,20>]");
        // A group with no complete note within the limit is left out.
        assert_eq!(shorten_speech("[:dv ap 200 hs 80 pr 250]", 10), "");
    }

    #[test]
    fn test_shorten_speech_result_is_always_balanced_and_within_the_limit() {
        let texts = [
            "Here is a song. [dah<600,20>dah<600,22>] and [:nh] words [dah<300,13>_<100>] end."
                .repeat(4),
            format!("[{}] Done. [:np]", "lah<250,25>".repeat(40)),
            "[:nh] One. [:nb] Two! [:nk] Three? [:np] Four. ".repeat(6),
        ];

        for text in texts.iter().map(|text| text.trim()) {
            assert!(is_balanced(text));
            for limit in 0..text.chars().count() + 2 {
                let shortened = shorten_speech(text, limit);
                assert!(shortened.chars().count() <= limit, "limit {limit}");
                assert!(is_balanced(&shortened), "limit {limit}: {shortened}");
                assert!(!shortened.ends_with(' '));
                // Notes are never split: every `<` has its `>`.
                assert_eq!(
                    shortened.matches('<').count(),
                    shortened.matches('>').count(),
                    "limit {limit}: {shortened}"
                );
            }
        }
    }

    #[test]
    fn test_shorten_speech_counts_characters_not_bytes() {
        let text = "é".repeat(30) + " " + &"é".repeat(30);
        assert_eq!(shorten_speech(&text, 40), "é".repeat(30));
    }

    #[test]
    fn test_prepare_answer_cleans_a_typical_answer() {
        assert_eq!(
            prepare_answer("  It\u{2019}s 42 \u{2014} obviously.\n\nNext line. 🙂 "),
            "It's 42 , obviously. Next line."
        );
    }

    #[test]
    fn test_prepare_answer_repairs_brackets_and_keeps_commands() {
        assert_eq!(
            prepare_answer("[:nh] Hello ] there [dah<600,20>"),
            "[:nh] Hello there [dah<600,20>]"
        );
    }

    #[test]
    fn test_prepare_answer_is_empty_when_nothing_is_speakable() {
        assert_eq!(prepare_answer(""), "");
        assert_eq!(prepare_answer(" \n 🙂 日本 "), "");
    }

    #[test]
    fn test_prepare_answer_enforces_the_ceiling() {
        // A spoken answer far over the limit is cut at a sentence.
        let long = "This is a sentence. ".repeat(MAX_TEXT_CHARS / 10);
        let answer = prepare_answer(&long);
        assert!(answer.chars().count() <= MAX_ANSWER_CHARS);
        assert!(answer.chars().count() > MAX_ANSWER_CHARS - 20);
        assert!(answer.ends_with("sentence."));

        // A song far over the limit is cut between notes, with its brackets intact.
        let song = "[dah<600,20>]".repeat(MAX_TEXT_CHARS / 5);
        let answer = prepare_answer(&song);
        assert!(answer.chars().count() <= MAX_ANSWER_CHARS);
        assert!(answer.chars().count() > MAX_ANSWER_CHARS - 13);
        assert!(answer.ends_with("[dah<600,20>]"));
        assert!(is_balanced(&answer));

        // Whatever comes out can be handed to `/say`'s validation with the prefix on.
        assert!(text::validate_text(&spoken_text(&answer)).is_ok());
    }
}
