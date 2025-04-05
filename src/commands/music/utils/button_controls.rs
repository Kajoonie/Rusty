use serenity::all::{ButtonStyle, CreateActionRow, CreateButton, ReactionType};

enum Emoji {
    Eject,
    Next,
    Pause,
    Play,
    Previous,
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
            Emoji::Previous => "⏮️",
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

#[derive(Clone, Debug)] // Add Clone and Debug derives
pub enum RepeatState {
    Disabled,
    RepeatOne,
}

/// Creates updated music control buttons based on player status
pub fn stateful_interaction_buttons(
    is_playing: bool,
    has_queue: bool,
    has_history: bool,
    no_track: bool,
    repeat_state: RepeatState,
    is_shuffle: bool,
) -> Vec<CreateActionRow> {
    let first_row = CreateActionRow::Buttons(vec![
        eject(),
        previous(has_history),
        play_pause(is_playing, no_track),
        next(is_playing, has_queue),
    ]);

    let second_row = CreateActionRow::Buttons(vec![
        search(),
        repeat(repeat_state),
        shuffle(is_shuffle),
        queue(has_queue),
    ]);

    vec![first_row, second_row]
}

fn previous(has_history: bool) -> CreateButton {
    CreateButton::new("music_previous")
        .emoji(Emoji::Previous)
        .style(ButtonStyle::Secondary)
        .disabled(!has_history)
}

fn play_pause(is_playing: bool, no_track: bool) -> CreateButton {
    CreateButton::new("music_play_pause")
        .emoji(if is_playing {
            Emoji::Pause
        } else {
            Emoji::Play
        })
        .style(ButtonStyle::Primary)
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
        RepeatState::RepeatOne => ButtonStyle::Success,
    };

    CreateButton::new("music_repeat")
        .emoji(Emoji::Repeat)
        .style(style)
        .disabled(false)
}

fn shuffle(active: bool) -> CreateButton {
    CreateButton::new("music_shuffle")
        .emoji(Emoji::Shuffle)
        .style(if active {
            ButtonStyle::Success
        } else {
            ButtonStyle::Secondary
        })
        .disabled(false)
}

fn queue(has_queue: bool) -> CreateButton {
    CreateButton::new("music_queue_toggle")
        .emoji(Emoji::Queue)
        .style(ButtonStyle::Secondary)
        .disabled(!has_queue) // Disable queue if nothing queued
}
