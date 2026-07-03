//! Anthropic Messages API → OpenAI Chat Completions API converter.
//!
//! Converts Anthropic request/response/streaming formats to OpenAI equivalents.

use crate::types::anthropic::{
    AnthropicContentBlock, AnthropicMessage, AnthropicRequest, AnthropicResponse, AnthropicUsage,
};
use crate::types::openai::{
    ChatCompletionsRequest, ChatCompletionsResponse, ChatChoice, ChatDelta, ChatFunction,
    ChatFunctionCall, ChatMessage, ChatTool, ChatToolCall, ChatUsage,
};
use chrono::Utc;
use serde_json::{json, Value};

/// Convert an Anthropic request to OpenAI Chat Completions format.
pub fn anthropic_to_chat_completions(req: &AnthropicRequest) -> Result<ChatCompletionsRequest, String> {
    let messages = anthropic_messages_to_chat_messages(&req.messages)?;

    let mut out = ChatCompletionsRequest {
        model: req.model.clone(),
        messages,
        stream: req.stream,
        temperature: req.temperature,
        top_p: req.top_p,
        max_tokens: req.max_tokens,
        ..Default::default()
    };

    // Handle system prompt
    if let Some(system) = &req.system {
        out.instructions = Some(anthropic_system_to_string(system));
    }

    // Handle tools
    if let Some(tools) = &req.tools {
        out.tools = Some(anthropic_tools_to_chat_tools(tools));
    }

    // Handle tool_choice
    if let Some(tc) = &req.tool_choice {
        out.tool_choice = Some(anthropic_tool_choice_to_openai(tc));
    }

    // Handle stop sequences
    if let Some(seqs) = &req.stop_sequences {
        out.stop = if seqs.len() == 1 {
            Some(json!(seqs[0].clone()))
        } else {
            Some(json!(seqs))
        };
    }

    Ok(out)
}

/// Convert Anthropic messages to OpenAI chat messages.
fn anthropic_messages_to_chat_messages(
    msgs: &[AnthropicMessage],
) -> Result<Vec<ChatMessage>, String> {
    let mut result = Vec::new();

    for msg in msgs {
        match msg.role.as_str() {
            "user" => {
                let content = anthropic_content_to_openai(&msg.content)?;
                result.push(ChatMessage {
                    role: "user".to_string(),
                    content: Some(content),
                    ..Default::default()
                });
            }
            "assistant" => {
                let (content, tool_calls, reasoning) =
                    anthropic_assistant_to_openai(&msg.content)?;
                let mut cm = ChatMessage {
                    role: "assistant".to_string(),
                    content: Some(content),
                    reasoning_content: reasoning,
                    ..Default::default()
                };
                if !tool_calls.is_empty() {
                    cm.tool_calls = Some(tool_calls);
                }
                result.push(cm);
            }
            _ => {
                result.push(ChatMessage {
                    role: msg.role.clone(),
                    content: Some(msg.content.clone()),
                    ..Default::default()
                });
            }
        }
    }

    Ok(result)
}

/// Convert Anthropic content (array of blocks or string) to OpenAI format.
fn anthropic_content_to_openai(content: &Value) -> Result<Value, String> {
    match content {
        Value::String(s) => Ok(json!(s)),
        Value::Array(blocks) => {
            let mut parts: Vec<Value> = Vec::new();
            for block in blocks {
                if let Some(obj) = block.as_object() {
                    let typ = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    match typ {
                        "text" => {
                            if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                                parts.push(json!({"type": "text", "text": text}));
                            }
                        }
                        "image" => {
                            if let Some(source) = obj.get("source") {
                                if let Some(data) = source.get("data").and_then(|v| v.as_str()) {
                                    let media_type = source
                                        .get("media_type")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("image/png");
                                    let url = format!("data:{};base64,{}", media_type, data);
                                    parts.push(json!({
                                        "type": "image_url",
                                        "image_url": {"url": url}
                                    }));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            if parts.len() == 1 {
                // Single text part - keep as simple string for compatibility
                if let Some(first) = parts.first() {
                    if let Some(text) = first.get("text").and_then(|v| v.as_str()) {
                        return Ok(json!(text));
                    }
                }
            }
            Ok(json!(parts))
        }
        _ => Ok(content.clone()),
    }
}

/// Convert assistant content to OpenAI format, extracting tool calls and reasoning.
fn anthropic_assistant_to_openai(
    content: &Value,
) -> Result<(Value, Vec<ChatToolCall>, Option<String>), String> {
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<ChatToolCall> = Vec::new();
    let mut reasoning: Option<String> = None;

    match content {
        Value::Array(blocks) => {
            for block in blocks {
                if let Some(obj) = block.as_object() {
                    match obj.get("type").and_then(|v| v.as_str()) {
                        Some("text") => {
                            if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                                text_parts.push(text.to_string());
                            }
                        }
                        Some("thinking") => {
                            if let Some(thinking) = obj.get("thinking").and_then(|v| v.as_str()) {
                                let current = reasoning.get_or_insert_with(String::new);
                                if !current.is_empty() {
                                    current.push('\n');
                                }
                                current.push_str(thinking);
                            }
                        }
                        Some("tool_use") => {
                            let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("");
                            let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("");
                            let input = obj.get("input").cloned().unwrap_or(Value::Null);
                            let args = if input == Value::Null {
                                "{}".to_string()
                            } else {
                                serde_json::to_string(&input).unwrap_or_default()
                            };
                            tool_calls.push(ChatToolCall {
                                id: Some(id.to_string()),
                                tool_type: Some("function".to_string()),
                                function: ChatFunctionCall {
                                    name: Some(name.to_string()),
                                    arguments: args,
                                },
                                index: None,
                            });
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => {
            if let Some(s) = content.as_str() {
                text_parts.push(s.to_string());
            }
        }
    }

    let text = text_parts.join("\n\n");
    let content_val = if text.is_empty() && tool_calls.is_empty() {
        json!("")
    } else if tool_calls.is_empty() {
        json!(text)
    } else {
        json!(text)
    };

    Ok((content_val, tool_calls, reasoning))
}

/// Convert Anthropic tools to OpenAI tools.
fn anthropic_tools_to_chat_tools(tools: &[crate::types::anthropic::AnthropicTool]) -> Vec<ChatTool> {
    tools
        .iter()
        .map(|t| ChatTool {
            tool_type: "function".to_string(),
            function: ChatFunction {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: Some(t.input_schema.clone()),
                strict: None,
            },
        })
        .collect()
}

/// Convert Anthropic tool_choice to OpenAI format.
fn anthropic_tool_choice_to_openai(tc: &Value) -> Value {
    match tc {
        Value::String(s) => tc.clone(), // "auto", "any", "none" pass through
        Value::Object(obj) => {
            if let Some(typ) = obj.get("type").and_then(|v| v.as_str()) {
                if typ == "tool" {
                    // Anthropic tool format → OpenAI function format
                    let name = obj.get("input")
                        .and_then(|i| i.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    json!({"type": "function", "function": {"name": name}})
                } else if typ == "function" {
                    let name = obj.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    json!({"type": "function", "function": {"name": name}})
                } else {
                    tc.clone()
                }
            } else {
                tc.clone()
            }
        }
        _ => tc.clone(),
    }
}

/// Convert Anthropic system prompt to a string for OpenAI instructions.
fn anthropic_system_to_string(system: &Value) -> String {
    match system {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => {
            let mut texts = Vec::new();
            for block in blocks {
                if let Some(obj) = block.as_object() {
                    if let (Some("text"), Some(text)) = (
                        obj.get("type").and_then(|v| v.as_str()),
                        obj.get("text").and_then(|v| v.as_str()),
                    ) {
                        texts.push(text);
                    }
                }
            }
            texts.join("\n\n")
        }
        _ => String::new(),
    }
}

/// Convert an Anthropic non-streaming response to OpenAI Chat Completions format.
pub fn anthropic_to_chat_response(
    resp: &AnthropicResponse,
    model: &str,
) -> Result<ChatCompletionsResponse, String> {
    let mut message = ChatMessage {
        role: "assistant".to_string(),
        content: Some(Value::String(String::new())),
        ..Default::default()
    };

    let mut tool_calls: Vec<ChatToolCall> = Vec::new();
    let mut reasoning: Option<String> = None;
    let mut text_parts: Vec<String> = Vec::new();

    for block in &resp.content {
        match block {
            AnthropicContentBlock::Text { text, .. } => {
                text_parts.push(text.clone());
            }
            AnthropicContentBlock::Thinking { thinking, .. } => {
                let current = reasoning.get_or_insert_with(String::new);
                if !current.is_empty() {
                    current.push('\n');
                }
                current.push_str(thinking);
            }
            AnthropicContentBlock::ToolUse { id, name, input, .. } => {
                let args = if let Some(v) = input {
                    serde_json::to_string(v).unwrap_or_default()
                } else {
                    "{}".to_string()
                };
                tool_calls.push(ChatToolCall {
                    id: Some(id.clone()),
                    tool_type: Some("function".to_string()),
                    function: ChatFunctionCall {
                        name: Some(name.clone()),
                        arguments: args,
                    },
                    index: None,
                });
            }
            _ => {}
        }
    }

    let text = text_parts.join("\n\n");
    message.content = if text.is_empty() && tool_calls.is_empty() {
        Some(json!(""))
    } else {
        Some(json!(text))
    };

    if let Some(r) = reasoning {
        message.reasoning_content = Some(r);
    }
    if !tool_calls.is_empty() {
        message.tool_calls = Some(tool_calls);
    }

    let finish_reason = match resp.stop_reason.as_str() {
        "tool_use" => "tool_calls",
        "max_tokens" => "length",
        _ => "stop",
    };

    let usage = anthropic_usage_to_chat_usage(&resp.usage);

    Ok(ChatCompletionsResponse {
        id: resp.id.clone(),
        object_type: "chat.completion".to_string(),
        created: Utc::now().timestamp(),
        model: model.to_string(),
        choices: vec![ChatChoice {
            index: 0,
            message,
            finish_reason: finish_reason.to_string(),
            delta: None,
        }],
        usage: Some(usage),
        system_fingerprint: None,
        service_tier: None,
    })
}

/// Convert Anthropic usage to OpenAI usage.
fn anthropic_usage_to_chat_usage(usage: &AnthropicUsage) -> ChatUsage {
    let cached = usage.cache_creation_input_tokens.unwrap_or(0)
        + usage.cache_read_input_tokens.unwrap_or(0);

    ChatUsage {
        prompt_tokens: usage.input_tokens,
        completion_tokens: usage.output_tokens,
        total_tokens: usage.input_tokens + usage.output_tokens,
        prompt_tokens_details: if cached > 0 {
            Some(crate::types::openai::PromptTokensDetails { cached_tokens: cached })
        } else {
            None
        },
    }
}
