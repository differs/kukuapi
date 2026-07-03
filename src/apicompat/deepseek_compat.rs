//! DeepSeek ↔ OpenAI format compatibility converters.
//!
//! DeepSeek uses an OpenAI-compatible API with extra fields (thinking, reasoning_effort).
//! This module handles translation between the two.

use crate::types::deepseek::{
    DeepSeekChatRequest, DeepSeekChatResponse, DeepSeekChatChunk, DeepSeekChoice, DeepSeekDelta,
    DeepSeekMessage, DeepSeekTool, DeepSeekUsage,
};
use crate::types::openai::{
    ChatCompletionsRequest, ChatCompletionsResponse, ChatCompletionsChunk, ChatChoice,
    ChatDelta, ChatMessage, ChatTool, ChatToolCall, ChatFunction, ChatFunctionCall, ChatUsage,
};
use chrono::Utc;
use serde_json::Value;

/// Convert DeepSeek request to OpenAI Chat Completions format.
pub fn deepseek_to_openai(req: &DeepSeekChatRequest) -> ChatCompletionsRequest {
    let messages = req
        .messages
        .iter()
        .map(|m| ChatMessage {
            role: m.role.clone(),
            content: Some(m.content.clone()),
            ..Default::default()
        })
        .collect();

    let tools = req.tools.as_ref().map(|ts| {
        ts.iter()
            .map(|t| ChatTool {
                tool_type: "function".to_string(),
                function: ChatFunction {
                    name: t.function.name.clone(),
                    description: t.function.description.clone(),
                    parameters: Some(t.function.parameters.clone()),
                    strict: None,
                },
            })
            .collect()
    });

    ChatCompletionsRequest {
        model: req.model.clone(),
        messages,
        stream: req.stream,
        temperature: req.temperature,
        top_p: req.top_p,
        max_tokens: req.max_tokens,
        stop: req.stop.clone(),
        tools,
        tool_choice: req.tool_choice.clone(),
        user: req.user.clone(),
        ..Default::default()
    }
}

/// Convert OpenAI Chat Completions request to DeepSeek format.
pub fn openai_to_deepseek(req: &ChatCompletionsRequest) -> DeepSeekChatRequest {
    let messages = req
        .messages
        .iter()
        .map(|m| DeepSeekMessage {
            role: m.role.clone(),
            content: m.content.clone().unwrap_or(Value::String(String::new())),
            name: m.name.clone(),
        })
        .collect();

    let tools = req.tools.as_ref().map(|ts| {
        ts.iter()
            .filter_map(|t| {
                if t.tool_type != "function" {
                    return None;
                }
                Some(DeepSeekTool {
                    tool_type: "function".to_string(),
                    function: crate::types::deepseek::DeepSeekFunction {
                        name: t.function.name.clone(),
                        description: t.function.description.clone(),
                        parameters: t.function.parameters.clone().unwrap_or(Value::Object(Default::default())),
                    },
                })
            })
            .collect()
    });

    DeepSeekChatRequest {
        model: req.model.clone(),
        messages,
        stream: req.stream,
        temperature: req.temperature,
        top_p: req.top_p,
        max_tokens: req.max_tokens,
        stop: req.stop.clone(),
        tools,
        tool_choice: req.tool_choice.clone(),
        thinking: req.reasoning_effort.as_ref().map(|_| crate::types::deepseek::DeepSeekThinking {
            thinking_type: "enabled".to_string(),
        }),
        reasoning_effort: req.reasoning_effort.clone(),
        user: req.user.clone(),
    }
}

/// Convert DeepSeek non-streaming response to OpenAI Chat Completions format.
pub fn deepseek_to_chat_response(resp: &DeepSeekChatResponse) -> ChatCompletionsResponse {
    let choices = resp
        .choices
        .iter()
        .map(|c| ChatChoice {
            index: c.index,
            message: ChatMessage {
                role: c.message.role.clone(),
                content: Some(c.message.content.clone()),
                reasoning_content: c.message.content.as_object()
                    .and_then(|o| o.get("reasoning_content"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                ..Default::default()
            },
            finish_reason: c.finish_reason.clone(),
            delta: None,
        })
        .collect();

    let usage = resp.usage.as_ref().map(|u| ChatUsage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
        prompt_tokens_details: u.prompt_details.as_ref().map(|pd| {
            crate::types::openai::PromptTokensDetails {
                cached_tokens: pd.cached_tokens,
            }
        }),
    });

    ChatCompletionsResponse {
        id: resp.id.clone(),
        object_type: "chat.completion".to_string(),
        created: resp.created,
        model: resp.model.clone(),
        choices,
        usage,
        system_fingerprint: None,
        service_tier: None,
    }
}

/// Convert OpenAI Chat Completions response to DeepSeek format.
pub fn chat_to_deepseek_response(resp: &ChatCompletionsResponse) -> DeepSeekChatResponse {
    let choices = resp
        .choices
        .iter()
        .map(|c| DeepSeekChoice {
            index: c.index,
            message: DeepSeekMessage {
                role: c.message.role.clone(),
                content: c.message.content.clone().unwrap_or(Value::String(String::new())),
                name: c.message.name.clone(),
            },
            finish_reason: c.finish_reason.clone(),
            delta: None,
        })
        .collect();

    let usage = resp.usage.as_ref().map(|u| DeepSeekUsage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
        prompt_details: u.prompt_tokens_details.as_ref().map(|pd| {
            crate::types::deepseek::DeepSeekPromptDetails {
                cached_tokens: pd.cached_tokens,
            }
        }),
    });

    DeepSeekChatResponse {
        id: resp.id.clone(),
        object_type: resp.object_type.clone(),
        created: resp.created,
        model: resp.model.clone(),
        choices,
        usage,
    }
}

/// Convert DeepSeek streaming chunk to OpenAI chunk.
pub fn deepseek_chunk_to_openai_chunk(chunk: &DeepSeekChatChunk) -> ChatCompletionsChunk {
    let choices = chunk
        .choices
        .iter()
        .map(|c| {
            let mut delta = ChatDelta::default();
            if let Some(ref d) = c.delta {
                if let Some(ref role) = d.role {
                    delta.role = Some(role.clone());
                }
                if let Some(ref content) = d.content {
                    delta.content = Some(Some(content.clone()));
                }
            }
            ChatChoice {
                index: c.index,
                message: ChatMessage::default(),
                finish_reason: c.finish_reason.clone(),
                delta: Some(delta),
            }
        })
        .collect();

    let usage = chunk.usage.as_ref().map(|u| ChatUsage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
        prompt_tokens_details: u.prompt_details.as_ref().map(|pd| {
            crate::types::openai::PromptTokensDetails {
                cached_tokens: pd.cached_tokens,
            }
        }),
    });

    ChatCompletionsChunk {
        id: chunk.id.clone(),
        object_type: "chat.completion.chunk".to_string(),
        created: chunk.created,
        model: chunk.model.clone(),
        choices,
        usage,
    }
}

/// Convert OpenAI streaming chunk to DeepSeek chunk.
pub fn openai_chunk_to_deepseek_chunk(chunk: &ChatCompletionsChunk) -> DeepSeekChatChunk {
    let choices = chunk
        .choices
        .iter()
        .map(|c| {
            let mut delta = DeepSeekDelta::default();
            if let Some(ref d) = c.delta {
                if let Some(ref role) = d.role {
                    delta.role = Some(role.clone());
                }
                if let Some(Some(ref content)) = d.content {
                    delta.content = Some(content.clone());
                }
            }
            DeepSeekChoice {
                index: c.index,
                message: DeepSeekMessage::default(),
                finish_reason: c.finish_reason.clone(),
                delta: Some(delta),
            }
        })
        .collect();

    let usage = chunk.usage.as_ref().map(|u| DeepSeekUsage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
        prompt_details: None,
    });

    DeepSeekChatChunk {
        id: chunk.id.clone(),
        object_type: chunk.object_type.clone(),
        created: chunk.created,
        model: chunk.model.clone(),
        choices,
        usage,
    }
}
