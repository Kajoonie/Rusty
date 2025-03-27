use serenity::all::{ButtonStyle, CreateActionRow, CreateButton, ReactionType};

/// Creates a row of music control buttons
pub fn create_music_control_buttons() -> Vec<CreateActionRow> {
    // Create buttons with appropriate styles and emojis
    let play_pause = CreateButton::new("music_play_pause")
        .emoji(ReactionType::Unicode("⏸️".to_string()))
        .style(ButtonStyle::Primary)
        .label("Pause");

    let stop = CreateButton::new("music_stop")
        .emoji(ReactionType::Unicode("⏹️".to_string()))
        .style(ButtonStyle::Danger)
        .label("Stop");

    let skip = CreateButton::new("music_skip")
        .emoji(ReactionType::Unicode("⏭️".to_string()))
        .style(ButtonStyle::Secondary)
        .label("Skip");

    // Create an action row containing our buttons
    vec![CreateActionRow::Buttons(vec![play_pause, stop, skip])]
}

/// Creates updated music control buttons based on player status
pub fn create_updated_buttons(is_playing: bool, has_queue: bool) -> Vec<CreateActionRow> {
    // Create buttons with appropriate states
    let play_pause = CreateButton::new("music_play_pause")
        .emoji(ReactionType::Unicode(
            if is_playing { "⏸️" } else { "▶️" }.to_string(),
        ))
        .style(ButtonStyle::Primary)
        .label(if is_playing { "Pause" } else { "Play" })
        .disabled(false);

    let stop = CreateButton::new("music_stop")
        .emoji(ReactionType::Unicode("⏹️".to_string()))
        .style(ButtonStyle::Danger)
        .label("Stop")
        .disabled(!is_playing && !has_queue);

    let skip = CreateButton::new("music_skip")
        .emoji(ReactionType::Unicode("⏭️".to_string()))
        .style(ButtonStyle::Secondary)
        .label("Skip")
        .disabled(!has_queue);

    // Create an action row containing our buttons
    vec![CreateActionRow::Buttons(vec![play_pause, stop, skip])]
}
