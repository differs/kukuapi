//! OpenAI Chat Completions API → Anthropic Messages API converter.
//!
//! Converts OpenAI request/response/streaming formats to Anthropic equivalents.

use crate::types::anthropic::{
    AnthropicContentBlock, AnthropicMessage, AnthropicRequest, AnthropicResponse, AnthropicTool,
    AnthropicUsage, AnthropicThinking, AnthropicOutputConfig,
};
use crate::types::openai::{
    ChatCompletionsRequest, ChatCompletionsResponse, ChatMessage as OpenAIMessage, ChatToolCall,
    ChatChoice,
};
use chrono::Utc;
use serde_json::{json, Value};

/// Convert an OpenAI Chat Completions request to Anthropic format.
pub fn chat_completions_to_anthropic(req: &ChatCompletionsRequest) -> Result<AnthropicRequest, String> {
    let mut messages = Vec::new();
    let mut system_value: Option<Value> = None;

    for msg in &req.messages {
        match msg.role.as_str() {
            "system" => {
                // Collect system messages for the system field
                let content = msg.content.as_ref().cloned().unwrap_or(json!(""));
                match &system_value {
                    Some(Value::Array(arr)) => {
                        let mut new_arr = arr.clone();
                        new_arr.push(content);
                        system_value = Some(json!(new_arr));
                    }
                    Some(Value::String(s)) => {
                        system_value = Some(json!([
                            json!({"type": "text", "text": s}),
                            content
                        ]));
                    }
                    _ => {
                        system_value = Some(content);
                    }
                }
            }
            "assistant" => {
                // Handle reasoning_content → thinking blocks
                let mut content = msg.content.clone().unwrap_or(json!(""));
                if let Some(ref reasoning) = msg.reasoning_content {
                    // If content is a string, convert to array with thinking + text
                    if let Some(text) = content.as_str() {
                        let mut blocks = Vec::new();
                        if !reasoning.is_empty() {
                            blocks.push(json!({
                                "type": "thinking",
                                "thinking": reasoning
                            }));
                        }
                        if !text.is_empty() {
                            blocks.push(json!({
                                "type": "text",
                                "text": text
                            }));
                        }
                        content = json!(blocks);
                    }
                }

                // Handle tool_calls
                let mut tool_calls = msg.tool_calls.clone().unwrap_or_default();
                if !tool_calls.is_empty() && content == json!("") {
                    // Convert tool_calls to tool_use blocks
                    let mut blocks: Vec<Value> = Vec::new();
                    if let Some(reasoning) = &msg.reasoning_content {
                        if !reasoning.is_empty() {
                            blocks.push(json!({
                                "type": "thinking",
                                "thinking": reasoning
                            }));
                        }
                    }
                    for (i, tc) in tool_calls.iter().enumerate() {
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.id.as_deref().unwrap_or(&format!("call_{}", i)),
                            "name": tc.function.name.as_deref().unwrap_or(""),
                            "input": parse_json_string(&tc.function.arguments)
                        }));
                    }
                    content = json!(blocks);
                }

                messages.push(AnthropicMessage {
                    role: "assistant".to_string(),
                    content,
                });
            }
            _ => {
                messages.push(AnthropicMessage {
                    role: msg.role.clone(),
                    content: msg.content.clone().unwrap_or(json!("")),
                });
            }
        }
    }

    // Convert tools
    let tools = req.tools.as_ref().map(|ts| {
        ts.iter()
            .filter_map(|t| {
                if t.tool_type != "function" {
                    return None;
                }
                Some(AnthropicTool {
                    name: t.function.name.clone(),
                    description: t.function.description.clone(),
                    input_schema: t.function.parameters.clone().unwrap_or(json!({})),
                    cache_control: None,
                })
            })
            .collect::<Vec<_>>()
    });

    // Convert tool_choice
    let tool_choice = req.tool_choice.clone();

    // Handle thinking/reasoning
    let thinking = if req.model.contains("reasoner") || req.reasoning_effort.is_some() {
        Some(AnthropicThinking {
            type_: Some("enabled".to_string()),
            budget_tokens: req.max_tokens,
        })
    } else {
        None
    };

    let stop = req.stop.as_ref().and_then(|v| {
        match v {
            Value::String(s) => Some(vec![s.clone()]),
            Value::Array(arr) => {
                Some(arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
            }
            _ => None,
        }
    });

    Ok(AnthropicRequest {
        model: req.model.clone(),
        max_tokens: req.max_tokens.or(req.max_completion_tokens),
        system: system_value,
        messages,
        tools,
        stream: req.stream,
        temperature: req.temperature,
        top_p: req.top_p,
        stop_sequences: stop,
        thinking,
        tool_choice,
        metadata: None,
        output_config: None,
    })
}

/// Parse a JSON string, returning the parsed Value or the raw string.
fn parse_json_string(s: &str) -> Value {
    if s.is_empty() {
        return json!({});
    }
    serde_json::from_str(s).unwrap_or(json!(s))
}

/// Convert an OpenAI Chat Completions response to Anthropic format.
pub fn chat_to_anthropic_response(
    resp: &ChatCompletionsResponse,
    model: &str,
) -> Result<AnthropicResponse, String> {
    let choice = resp.choices.first().ok_or("No choices in response")?;
    let msg = &choice.message;

    let mut content_blocks: Vec<AnthropicContentBlock> = Vec::new();

    // Handle reasoning_content → thinking
    if let Some(reasoning) = &msg.reasoning_content {
        if !reasoning.is_empty() {
            content_blocks.push(AnthropicContentBlock::Thinking {
                thinking: reasoning.clone(),
                cache_control: None,
            });
        }
    }

    // Handle content
    if let Some(ref content) = msg.content {
        match content {
            Value::String(s) => {
                if !s.is_empty() {
                    content_blocks.push(AnthropicContentBlock::Text {
                        text: s.clone(),
                        cache_control: None,
                    });
                }
            }
            Value::Array(parts) => {
                for part in parts {
                    if let Some(obj) = part.as_object() {
                        match obj.get("type").and_then(|v| v.as_str()) {
                            Some("text") => {
                                if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                                    content_blocks.push(AnthropicContentBlock::Text {
                                        text: text.to_string(),
                                        cache_control: None,
                                    });
                                }
                            }
                            Some("image_url") => {
                                if let Some(img_obj) = obj.get("image_url") {
                                    if let Some(url) = img_obj.get("url").and_then(|v| v.as_str()) {
                                        if url.starts_with("data:") {
                                            // Parse data URI
                                            let parts: Vec<&str> = url.split(',').collect();
                                            if parts.len() == 2 {
                                                let meta = parts[0];
                                                let data = parts[1];
                                                let media_type = meta
                                                    .strip_prefix("data:")
                                                    .and_then(|m| m.split_once(';'))
                                                    .map(|(mt, _)| mt)
                                                    .unwrap_or("image/png");
                                                content_blocks.push(AnthropicContentBlock::Image {
                                                    source: crate::types::anthropic::AnthropicImageSource {
                                                        source_type: "base64".to_string(),
                                                        media_type: media_type.to_string(),
                                                        data: data.to_string(),
                                                    },
                                                    cache_control: None,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Handle tool_calls
    if let Some(ref tool_calls) = msg.tool_calls {
        for tc in tool_calls {
            let input = parse_json_string(&tc.function.arguments);
            content_blocks.push(AnthropicContentBlock::ToolUse {
                id: tc.id.clone().unwrap_or_default(),
                name: tc.function.name.clone().unwrap_or_default(),
                input: Some(input),
            });
        }
    }

    if content_blocks.is_empty() {
        content_blocks.push(AnthropicContentBlock::Text {
            text: String::new(),
            cache_control: None,
        });
    }

    let stop_reason = match choice.finish_reason.as_str() {
        "tool_calls" => "tool_use",
        "length" => "max_tokens",
        "content_filter" => "stop_sequence",
        _ => "end_turn",
    };

    let usage = resp.usage.as_ref().map(|u| AnthropicUsage {
        input_tokens: u.prompt_tokens,
        output_tokens: u.completion_tokens,
        cache_creation_input_tokens: u.prompt_tokens_details.as_ref().map(|d| d.cached_tokens),
        cache_read_input_tokens: None,
    });

    // Generate a Claude-style ID
    let id = resp.id.clone();

    Ok(AnthropicResponse {
        id,
        response_type: "message".to_string(),
        role: "assistant".to_string(),
        content: content_blocks,
        model: model.to_string(),
        stop_reason: stop_reason.to_string(),
        stop_sequence: None,
        usage: usage.unwrap_or(AnthropicUsage {
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_input_tokens: None,
            cache_read_input_tokens: None,
        }),
    })
}
