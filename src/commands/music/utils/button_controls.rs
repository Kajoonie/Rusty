use serenity::all::{ButtonStyle, CreateActionRow, CreateButton, ReactionType};

enum Emoji {
    Play,
    Pause,
    Stop,
    Previous,
    Next,
    Queue,
}

impl From<Emoji> for String {
    fn from(value: Emoji) -> Self {
        let emoji = match value {
            Emoji::Play => "▶️",
            Emoji::Pause => "⏸️",
            Emoji::Stop => "⏹️",
            Emoji::Previous => "⏮️",
            Emoji::Next => "⏭️",
            Emoji::Queue => "📜",
        };
        return emoji.to_string();
    }
}

impl From<Emoji> for ReactionType {
    fn from(value: Emoji) -> Self {
        ReactionType::Unicode(value.into())
    }
}

/// Creates updated music control buttons based on player status
pub fn create_updated_buttons(
    is_playing: bool,
    has_queue: bool,
    has_history: bool,
    no_track: bool,
) -> Vec<CreateActionRow> {
    // Create buttons with appropriate states
    let previous = CreateButton::new("music_previous")
        .emoji(Emoji::Previous)
        .style(ButtonStyle::Secondary)
        .disabled(!has_history);

    let play_pause = CreateButton::new("music_play_pause")
        .emoji(if is_playing {
            Emoji::Pause
        } else {
            Emoji::Play
        })
        .style(ButtonStyle::Primary)
        .disabled(no_track); // Disable play/pause if there's no track

    let stop = CreateButton::new("music_stop")
        // .emoji(ReactionType::Unicode("⏹️".to_string()))
        .emoji(Emoji::Stop)
        .style(ButtonStyle::Danger)
        .disabled(false); // Disable stop if nothing playing/queued

    let skip = CreateButton::new("music_next")
        // .emoji(ReactionType::Unicode("⏭️".to_string()))
        .emoji(Emoji::Next)
        .style(ButtonStyle::Secondary)
        .disabled(!is_playing && !has_queue); // Disable skip if nothing playing and no queue

    let queue = CreateButton::new("music_queue_toggle")
        // .emoji(ReactionType::Unicode("📜".to_string()))
        .emoji(Emoji::Queue)
        .style(ButtonStyle::Secondary)
        .label("Queue")
        .disabled(!is_playing && !has_queue); // Disable queue if nothing playing/queued

    // Create an action row containing our buttons
    vec![CreateActionRow::Buttons(vec![
        previous, play_pause, stop, skip, queue,
    ])]
}
