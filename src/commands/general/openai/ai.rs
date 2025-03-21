use std::sync::Mutex;
use dashmap::DashMap;
use once_cell::sync::Lazy;

use ollama_rs::error::OllamaError;
use ollama_rs::generation::chat::request::ChatMessageRequest;
use ollama_rs::Ollama;
use ollama_rs::generation::chat::{ChatMessage, ChatMessageResponse};

use crate::database::{self, UserPreference};
use super::*;

static CONVO_MAP: Lazy<DashMap<String, Mutex<Vec<ChatMessage>>>> = Lazy::new(DashMap::new);

fn get_conversation_history(user: &str) -> Vec<ChatMessage> {
    CONVO_MAP.entry(user.to_string()).or_insert_with(|| {
        Mutex::new(vec![])
    });

    let user_convo = CONVO_MAP.get(user).unwrap();
    let messages = user_convo.lock().unwrap();
    messages.clone()
}

#[poise::command(slash_command, category = "General", subcommands("chat", "list_models", "set_model", "get_model"))]
pub async fn ai(
    _ctx: Context<'_>
) -> CommandResult {
    Ok(())
}

#[poise::command(slash_command, category = "General")]
pub async fn chat(
    ctx: Context<'_>,
    #[description = "Your chat message"]
    #[rest]
    message: String,
) -> CommandResult {
    ctx.defer().await?;

    let author = ctx.author();
    let author_str = format!("{}{}", author.name, author.id);
    let model = database::get_user_model(&author.id.to_string());

    let chat_history = get_conversation_history(&author_str);
    let response = send_request_with_model(message.clone(), chat_history, &model).await?;

    let content = response.message.content;
    let full_message = format!("**{}**: {message}\n\n**AI**: {content}", author.name);

    chunk_response(ctx, full_message).await
}

async fn send_request_with_model(user_message: String, mut chat_history: Vec<ChatMessage>, model: &str) -> Result<ChatMessageResponse, OllamaError> {
    let mut ollama = Ollama::default();

    ollama.send_chat_messages_with_history(
            &mut chat_history,
            ChatMessageRequest::new(
                model.to_string(),
                vec![ChatMessage::user(user_message)],
            )
        )
        .await
}

#[poise::command(slash_command, category = "General")]
pub async fn list_models(ctx: Context<'_>) -> CommandResult {
    ctx.defer().await?;

    let ollama = Ollama::default();
    let models = ollama.list_local_models().await?;

    let mut model_list = "Here are the available models:\n".to_string();
    for model in models.iter() {
        model_list.push_str(&format!("- {}\n", model.name));
    }

    ctx.say(model_list).await?;
    Ok(())
}

#[poise::command(slash_command, category = "General")]
pub async fn set_model(
    ctx: Context<'_>,
    #[description = "The model to use"] model: String,
) -> CommandResult {
    ctx.defer().await?;
    
    let ollama = Ollama::default();
    let models = ollama.list_local_models().await?;
    
    if !models.iter().any(|m| m.name == model) {
        ctx.say(format!("Model '{}' is not available. Use `/ai list_models` to see available models.", model)).await?;
        return Ok(());
    }

    let author = ctx.author();
    let pref = UserPreference {
        user_id: author.id.to_string(),
        username: author.name.clone(),
        model: model.clone(),
    };

    match database::set_user_preference(&pref) {
        Ok(_) => {
            ctx.say(format!("Your preferred model has been set to '{}'", model)).await?;
        }
        Err(e) => {
            ctx.say(format!("Failed to set model preference: {}", e)).await?;
        }
    }

    Ok(())
}

#[poise::command(slash_command, category = "General")]
pub async fn get_model(ctx: Context<'_>) -> CommandResult {
    ctx.defer().await?;

    let author = ctx.author();
    let model = database::get_user_model(&author.id.to_string());

    ctx.say(format!("Your currently active model is: **{}**", model)).await?;
    Ok(())
}