//! Streaming event conversion utilities.
//!
//! Converts SSE events between Anthropic and OpenAI Chat Completions streaming formats.

use crate::types::anthropic::AnthropicStreamEvent;
use crate::types::openai::{ChatCompletionsChunk, ChatChoice, ChatDelta};
use chrono::Utc;
use serde_json::Value;

/// Convert an Anthropic stream event to an OpenAI Chat Completions chunk.
pub fn anthropic_chunk_to_openai_chunk(
    event: &AnthropicStreamEvent,
) -> Result<ChatCompletionsChunk, String> {
    let mut delta = ChatDelta::default();

    if let Some(ref data) = event.data {
        match event.event_type.as_str() {
            "content_block_delta" => {
                if let Some(d) = data.get("delta") {
                    if let Some(text) = d.get("text").and_then(|v| v.as_str()) {
                        delta.content = Some(Some(text.to_string()));
                    }
                    if let Some(thinking) = d.get("thinking").and_then(|v| v.as_str()) {
                        delta.reasoning_content = Some(thinking.to_string());
                    }
                }
            }
            "message_delta" => {
                if let Some(d) = data.get("delta") {
                    if let Some(fr) = d.get("stop_reason").and_then(|v| v.as_str()) {
                        delta.content = Some(Some(String::new()));
                        return Ok(ChatCompletionsChunk {
                            id: "chatcmpl-temp".to_string(),
                            object_type: "chat.completion.chunk".to_string(),
                            created: Utc::now().timestamp(),
                            model: "unknown".to_string(),
                            choices: vec![ChatChoice {
                                index: 0,
                                message: Default::default(),
                                finish_reason: match fr {
                                    "end_turn" | "stop_sequence" => "stop".to_string(),
                                    "max_tokens" => "length".to_string(),
                                    "tool_use" => "tool_calls".to_string(),
                                    _ => fr.to_string(),
                                },
                                delta: Some(delta),
                            }],
                            usage: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    Ok(ChatCompletionsChunk {
        id: "chatcmpl-temp".to_string(),
        object_type: "chat.completion.chunk".to_string(),
        created: Utc::now().timestamp(),
        model: "unknown".to_string(),
        choices: vec![ChatChoice {
            index: 0,
            message: Default::default(),
            finish_reason: String::new(),
            delta: Some(delta),
        }],
        usage: None,
    })
}

/// Convert an OpenAI Chat Completions chunk to an Anthropic stream event.
pub fn openai_chunk_to_anthropic_chunk(
    chunk: &ChatCompletionsChunk,
) -> Result<AnthropicStreamEvent, String> {
    if chunk.choices.is_empty() {
        return Ok(AnthropicStreamEvent {
            event_type: "error".to_string(),
            data: Some(Value::String("empty chunk".to_string())),
        });
    }

    let choice = &chunk.choices[0];
    let default_delta = ChatDelta::default();
    let delta = choice.delta.as_ref().unwrap_or(&default_delta);

    let mut event_type = "content_block_delta".to_string();
    let mut data = Value::Object(serde_json::Map::new());

    if !choice.finish_reason.is_empty() && choice.finish_reason != "null" {
        event_type = "message_delta".to_string();
        let mut delta_obj = serde_json::Map::new();
        delta_obj.insert(
            "stop_reason".to_string(),
            match choice.finish_reason.as_str() {
                "tool_calls" => Value::String("tool_use".to_string()),
                "length" => Value::String("max_tokens".to_string()),
                _ => Value::String("end_turn".to_string()),
            },
        );
        data = Value::Object(delta_obj);
    } else if let Some(Some(ref content)) = delta.content {
        if !content.is_empty() {
            let mut delta_obj = serde_json::Map::new();
            delta_obj.insert("type".to_string(), Value::String("text_delta".to_string()));
            delta_obj.insert("text".to_string(), Value::String(content.clone()));
            data = Value::Object(delta_obj);
        }
    } else if let Some(ref reasoning) = delta.reasoning_content {
        if !reasoning.is_empty() {
            let mut delta_obj = serde_json::Map::new();
            delta_obj.insert("type".to_string(), Value::String("thinking_delta".to_string()));
            delta_obj.insert("thinking".to_string(), Value::String(reasoning.clone()));
            data = Value::Object(delta_obj);
        }
    }

    Ok(AnthropicStreamEvent {
        event_type,
        data: if data.is_null() { None } else { Some(data) },
    })
}
