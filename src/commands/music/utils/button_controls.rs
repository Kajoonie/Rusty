//! Defines the creation logic for the interactive music control buttons.
//! Includes button styles, emojis, and state-dependent enabling/disabling.

use serenity::all::{ButtonStyle, CreateActionRow, CreateButton, ReactionType};

/// Enum representing the emojis used for different control buttons.
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

/// Converts an `Emoji` variant into its corresponding Unicode string representation.
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

/// Converts an `Emoji` variant into a `serenity::all::ReactionType` (specifically Unicode).
impl From<Emoji> for ReactionType {
    fn from(value: Emoji) -> Self {
        ReactionType::Unicode(value.into())
    }
}

/// Represents the possible states for the repeat function.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum RepeatState {
    /// Repeat is off.
    Disabled,
    /// Repeat the current track.
    Track,
}

/// Struct holding the necessary state information to determine button appearance and enabled status.
pub struct ButtonData {
    /// Is a track currently playing?
    pub is_playing: bool,
    /// Are there tracks in the queue (excluding the currently playing one, if any)?
    pub has_queue: bool,
    /// Is the queue currently being displayed?
    pub show_queue: bool,
    /// Is there no track currently loaded/playing?
    pub no_track: bool,
    /// The current repeat state.
    pub repeat_state: RepeatState,
}

/// Generates the `CreateActionRow` components containing the music control buttons.
/// The appearance and enabled state of buttons depend on the provided `ButtonData`.
pub fn stateful_interaction_buttons(data: ButtonData) -> Vec<CreateActionRow> {
    // First row: Eject, Play/Pause, Next
    let first_row = CreateActionRow::Buttons(vec![
        eject(),
        play_pause(data.is_playing, data.no_track),
        next(data.is_playing, data.has_queue),
    ]);

    // Second row: Search, Repeat, Shuffle, Queue Toggle
    let second_row = CreateActionRow::Buttons(vec![
        search(),
        repeat(data.repeat_state),
        shuffle(),
        queue(data.has_queue, data.show_queue),
    ]);

    vec![first_row, second_row]
}

/// Creates the Play/Pause button.
/// Shows Pause icon (Primary style) if playing, Play icon (Secondary style) otherwise.
/// Disabled if `no_track` is true.
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

/// Creates the Eject (Stop/Leave) button.
/// Always enabled (logic for leaving channel might be handled elsewhere).
fn eject() -> CreateButton {
    CreateButton::new("music_eject")
        .emoji(Emoji::Eject)
        .style(ButtonStyle::Danger)
        .disabled(false) // Disable stop if nothing playing/queued
}

/// Creates the Next (Skip) button.
/// Disabled if nothing is playing AND the queue is empty.
fn next(is_playing: bool, has_queue: bool) -> CreateButton {
    CreateButton::new("music_next")
        .emoji(Emoji::Next)
        .style(ButtonStyle::Secondary)
        .disabled(!is_playing && !has_queue) // Disable skip if nothing playing and no queue
}

/// Creates the Search button.
/// Always enabled.
fn search() -> CreateButton {
    CreateButton::new("music_search")
        .emoji(Emoji::Search)
        .style(ButtonStyle::Secondary)
        .disabled(false)
}

/// Creates the Repeat button.
/// Style changes based on the `RepeatState` (Primary if repeating track, Secondary otherwise).
/// Always enabled.
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

/// Creates the Shuffle button.
/// Always enabled (shuffling an empty/single-track queue has no effect).
fn shuffle() -> CreateButton {
    CreateButton::new("music_shuffle")
        .emoji(Emoji::Shuffle)
        .style(ButtonStyle::Secondary)
        .disabled(false)
}

/// Creates the Queue Toggle button.
/// Style changes based on `show_queue` (Primary if shown, Secondary otherwise).
/// Disabled if `has_queue` is false.
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
