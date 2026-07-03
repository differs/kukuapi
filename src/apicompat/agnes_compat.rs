//! Agnes ↔ OpenAI format compatibility converters.
//!
//! Agnes uses an OpenAI-compatible API with Agnes-specific fields (metadata, agent_id).
//! This module handles translation between the two.

use crate::types::agnes::{
    AgnesChatRequest, AgnesChatResponse, AgnesChatChunk, AgnesChoice, AgnesDelta,
    AgnesMessage, AgnesTool, AgnesUsage,
};
use crate::types::openai::{
    ChatCompletionsRequest, ChatCompletionsResponse, ChatCompletionsChunk, ChatChoice,
    ChatDelta, ChatMessage, ChatTool, ChatFunction, ChatFunctionCall, ChatUsage,
};
use chrono::Utc;
use serde_json::Value;

/// Convert Agnes request to OpenAI Chat Completions format.
pub fn agnes_to_openai(req: &AgnesChatRequest) -> ChatCompletionsRequest {
    let messages = req
        .messages
        .iter()
        .map(|m| ChatMessage {
            role: m.role.clone(),
            content: Some(m.content.clone()),
            name: m.name.clone(),
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
        ..Default::default()
    }
}

/// Convert OpenAI Chat Completions request to Agnes format.
pub fn openai_to_agnes(req: &ChatCompletionsRequest) -> AgnesChatRequest {
    let messages = req
        .messages
        .iter()
        .map(|m| AgnesMessage {
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
                Some(AgnesTool {
                    tool_type: "function".to_string(),
                    function: crate::types::agnes::AgnesFunction {
                        name: t.function.name.clone(),
                        description: t.function.description.clone(),
                        parameters: t.function.parameters.clone().unwrap_or(Value::Object(Default::default())),
                    },
                })
            })
            .collect()
    });

    AgnesChatRequest {
        model: req.model.clone(),
        messages,
        stream: req.stream,
        temperature: req.temperature,
        top_p: req.top_p,
        max_tokens: req.max_tokens,
        stop: req.stop.clone(),
        tools,
        tool_choice: req.tool_choice.clone(),
        metadata: None,
        agent_id: None,
        extra_body: None,
    }
}

/// Convert Agnes non-streaming response to OpenAI format.
pub fn agnes_to_chat_response(resp: &AgnesChatResponse) -> ChatCompletionsResponse {
    let choices = resp
        .choices
        .iter()
        .map(|c| ChatChoice {
            index: c.index,
            message: ChatMessage {
                role: c.message.role.clone(),
                content: Some(c.message.content.clone()),
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
        prompt_tokens_details: None,
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

/// Convert OpenAI Chat Completions response to Agnes format.
pub fn chat_to_agnes_response(resp: &ChatCompletionsResponse) -> AgnesChatResponse {
    let choices = resp
        .choices
        .iter()
        .map(|c| AgnesChoice {
            index: c.index,
            message: AgnesMessage {
                role: c.message.role.clone(),
                content: c.message.content.clone().unwrap_or(Value::String(String::new())),
                name: c.message.name.clone(),
            },
            finish_reason: c.finish_reason.clone(),
            delta: None,
        })
        .collect();

    let usage = resp.usage.as_ref().map(|u| AgnesUsage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
    });

    AgnesChatResponse {
        id: resp.id.clone(),
        object_type: resp.object_type.clone(),
        created: resp.created,
        model: resp.model.clone(),
        choices,
        usage,
        agnes_meta: None,
    }
}

/// Convert Agnes streaming chunk to OpenAI chunk.
pub fn agnes_chunk_to_openai_chunk(chunk: &AgnesChatChunk) -> ChatCompletionsChunk {
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
        prompt_tokens_details: None,
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

/// Convert OpenAI streaming chunk to Agnes chunk.
pub fn openai_chunk_to_agnes_chunk(chunk: &ChatCompletionsChunk) -> AgnesChatChunk {
    let choices = chunk
        .choices
        .iter()
        .map(|c| {
            let mut delta = AgnesDelta::default();
            if let Some(ref d) = c.delta {
                if let Some(ref role) = d.role {
                    delta.role = Some(role.clone());
                }
                if let Some(Some(ref content)) = d.content {
                    delta.content = Some(content.clone());
                }
            }
            AgnesChoice {
                index: c.index,
                message: AgnesMessage {
                    role: String::new(),
                    content: Value::Null,
                    name: None,
                },
                finish_reason: c.finish_reason.clone(),
                delta: Some(delta),
            }
        })
        .collect();

    let usage = chunk.usage.as_ref().map(|u| AgnesUsage {
        prompt_tokens: u.prompt_tokens,
        completion_tokens: u.completion_tokens,
        total_tokens: u.total_tokens,
    });

    AgnesChatChunk {
        id: chunk.id.clone(),
        object_type: chunk.object_type.clone(),
        created: chunk.created,
        model: chunk.model.clone(),
        choices,
        usage,
    }
}
