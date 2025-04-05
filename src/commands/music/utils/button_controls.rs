use serenity::all::{ButtonStyle, CreateActionRow, CreateButton, ReactionType};

enum Emoji {
    Eject,
    Next,
    Pause,
    Play,
    Queue,
    Repeat,
    Search,
    Shuffle,
}

impl From<Emoji> for String {
    fn from(value: Emoji) -> Self {
        let emoji = match value {
            Emoji::Eject => "⏏️",
            Emoji::Next => "⏭️",
            Emoji::Pause => "⏸️",
            Emoji::Play => "▶️",
            Emoji::Queue => "📃",
            Emoji::Repeat => "🔂",
            Emoji::Search => "🔍",
            Emoji::Shuffle => "🔀",
        };
        emoji.to_string()
    }
}

impl From<Emoji> for ReactionType {
    fn from(value: Emoji) -> Self {
        ReactionType::Unicode(value.into())
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RepeatState {
    Disabled,
    Track,
}

pub struct ButtonData {
    pub is_playing: bool,
    pub has_queue: bool,
    pub show_queue: bool,
    pub no_track: bool,
    pub repeat_state: RepeatState,
}

/// Creates updated music control buttons based on player status
pub fn stateful_interaction_buttons(data: ButtonData) -> Vec<CreateActionRow> {
    let first_row = CreateActionRow::Buttons(vec![
        eject(),
        play_pause(data.is_playing, data.no_track),
        next(data.is_playing, data.has_queue),
    ]);

    let second_row = CreateActionRow::Buttons(vec![
        search(),
        repeat(data.repeat_state),
        shuffle(),
        queue(data.has_queue, data.show_queue),
    ]);

    vec![first_row, second_row]
}

fn play_pause(is_playing: bool, no_track: bool) -> CreateButton {
    let (emoji, style) = match is_playing {
        true => (Emoji::Pause, ButtonStyle::Primary),
        false => (Emoji::Play, ButtonStyle::Secondary),
    };

    CreateButton::new("music_play_pause")
        .emoji(emoji)
        .style(style)
        .disabled(no_track) // Disable play/pause if there's no track
}

fn eject() -> CreateButton {
    CreateButton::new("music_eject")
        .emoji(Emoji::Eject)
        .style(ButtonStyle::Danger)
        .disabled(false) // Disable stop if nothing playing/queued
}

fn next(is_playing: bool, has_queue: bool) -> CreateButton {
    CreateButton::new("music_next")
        .emoji(Emoji::Next)
        .style(ButtonStyle::Secondary)
        .disabled(!is_playing && !has_queue) // Disable skip if nothing playing and no queue
}

fn search() -> CreateButton {
    CreateButton::new("music_search")
        .emoji(Emoji::Search)
        .style(ButtonStyle::Secondary)
        .disabled(false)
}

fn repeat(state: RepeatState) -> CreateButton {
    let style = match state {
        RepeatState::Disabled => ButtonStyle::Secondary,
        RepeatState::Track => ButtonStyle::Primary,
    };

    CreateButton::new("music_repeat")
        .emoji(Emoji::Repeat)
        .style(style)
        .disabled(false)
}

fn shuffle() -> CreateButton {
    CreateButton::new("music_shuffle")
        .emoji(Emoji::Shuffle)
        .style(ButtonStyle::Secondary)
        .disabled(false)
}

fn queue(has_queue: bool, show_queue: bool) -> CreateButton {
    let style = match show_queue {
        true => ButtonStyle::Primary,
        false => ButtonStyle::Secondary,
    };

    CreateButton::new("music_queue_toggle")
        .emoji(Emoji::Queue)
        .style(style)
        .disabled(!has_queue) // Disable queue if nothing queued
}
