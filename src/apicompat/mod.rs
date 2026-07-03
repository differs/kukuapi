//! API format conversion utilities.
//!
//! Provides bidirectional conversion between:
//! - Anthropic Messages API ↔ OpenAI Chat Completions API
//! - OpenAI Chat Completions API ↔ OpenAI Responses API
//! - DeepSeek format ↔ OpenAI format
//! - Agnes format ↔ OpenAI format

pub mod anthropic_to_openai;
pub mod openai_to_anthropic;
pub mod deepseek_compat;
pub mod agnes_compat;
pub mod streaming;

pub use anthropic_to_openai::*;
pub use openai_to_anthropic::*;
pub use deepseek_compat::*;
pub use agnes_compat::*;
pub use streaming::*;

use crate::types::openai::{
    ChatCompletionsRequest, ChatCompletionsResponse, ChatMessage, ChatTool, ChatFunction,
    ResponsesRequest, ResponsesResponse, ResponsesOutput, ResponsesContentPart,
    ResponsesIncompleteDetails, ResponsesInputTokensDetails, ResponsesUsage,
};

/// Unified request enum covering all supported input formats.
#[derive(Debug, Clone)]
pub enum ApiRequest {
    Anthropic(crate::types::anthropic::AnthropicRequest),
    OpenAIChat(crate::types::openai::ChatCompletionsRequest),
    OpenAIResponses(crate::types::openai::ResponsesRequest),
    DeepSeek(crate::types::deepseek::DeepSeekChatRequest),
    Agnes(crate::types::agnes::AgnesChatRequest),
}

impl ApiRequest {
    /// Extract the model name from any variant.
    pub fn model_name(&self) -> &str {
        match self {
            ApiRequest::Anthropic(r) => &r.model,
            ApiRequest::OpenAIChat(r) => &r.model,
            ApiRequest::OpenAIResponses(r) => &r.model,
            ApiRequest::DeepSeek(r) => &r.model,
            ApiRequest::Agnes(r) => &r.model,
        }
    }
}

/// Unified response enum covering all supported output formats.
#[derive(Debug, Clone)]
pub enum ApiResponse {
    Anthropic(crate::types::anthropic::AnthropicResponse),
    OpenAIChat(crate::types::openai::ChatCompletionsResponse),
    OpenAIResponses(crate::types::openai::ResponsesResponse),
    DeepSeek(crate::types::deepseek::DeepSeekChatResponse),
    Agnes(crate::types::agnes::AgnesChatResponse),
}

/// Unified streaming chunk.
#[derive(Debug, Clone)]
pub enum ApiStreamChunk {
    Anthropic(crate::types::anthropic::AnthropicStreamEvent),
    OpenAIChat(crate::types::openai::ChatCompletionsChunk),
    OpenAIResponses(crate::types::openai::ResponsesStreamEvent),
    DeepSeek(crate::types::deepseek::DeepSeekChatChunk),
    Agnes(crate::types::agnes::AgnesChatChunk),
}

/// Target platform for forwarding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Anthropic,
    OpenAI,
    DeepSeek,
    Agnes,
    Gemini,
}

impl Platform {
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Anthropic => "anthropic",
            Platform::OpenAI => "openai",
            Platform::DeepSeek => "deepseek",
            Platform::Agnes => "agnes",
            Platform::Gemini => "gemini",
        }
    }
}

/// Convert an incoming request to the target platform format.
pub fn convert_request(
    request: &ApiRequest,
    target: Platform,
) -> Result<ApiRequest, String> {
    match (request, target) {
        (ApiRequest::Anthropic(req), Platform::OpenAI) => {
            Ok(ApiRequest::OpenAIChat(anthropic_to_chat_completions(req)?))
        }
        (ApiRequest::OpenAIChat(req), Platform::Anthropic) => {
            Ok(ApiRequest::Anthropic(chat_completions_to_anthropic(req)?))
        }
        (ApiRequest::DeepSeek(req), Platform::OpenAI) => {
            Ok(ApiRequest::OpenAIChat(deepseek_to_openai(req)))
        }
        (ApiRequest::OpenAIChat(req), Platform::DeepSeek) => {
            Ok(ApiRequest::DeepSeek(openai_to_deepseek(req)))
        }
        (ApiRequest::Agnes(req), Platform::OpenAI) => {
            Ok(ApiRequest::OpenAIChat(agnes_to_openai(req)))
        }
        (ApiRequest::OpenAIChat(req), Platform::Agnes) => {
            // Agnes API is OpenAI-compatible, pass through as-is
            Ok(ApiRequest::OpenAIChat(req.clone()))
        }
        (ApiRequest::Anthropic(_), Platform::Anthropic) => Ok(request.clone()),
        (ApiRequest::OpenAIChat(_), Platform::OpenAI) => Ok(request.clone()),
        (ApiRequest::DeepSeek(_), Platform::DeepSeek) => Ok(request.clone()),
        (ApiRequest::Agnes(_), Platform::Agnes) => Ok(request.clone()),
        (_, Platform::Gemini) => {
            Err("Gemini forwarding requires OAuth token management".to_string())
        }
        _ => Err(format!(
            "Unsupported conversion: {:?} → {:?}",
            request, target
        )),
    }
}

/// Convert a non-streaming response to the target output format.
pub fn convert_response(
    response: &ApiResponse,
    target_format: OutputFormat,
    _model: &str,
) -> Result<ApiResponse, String> {
    match (response, target_format) {
        (ApiResponse::Anthropic(resp), OutputFormat::OpenAIChat) => {
            Ok(ApiResponse::OpenAIChat(anthropic_to_chat_response(resp, _model)?))
        }
        (ApiResponse::Anthropic(resp), OutputFormat::Anthropic) => Ok(response.clone()),
        (ApiResponse::OpenAIChat(resp), OutputFormat::Anthropic) => {
            Ok(ApiResponse::Anthropic(chat_to_anthropic_response(resp, _model)?))
        }
        (ApiResponse::OpenAIChat(resp), OutputFormat::OpenAIChat) => Ok(response.clone()),
        (ApiResponse::OpenAIChat(resp), OutputFormat::DeepSeek) => {
            Ok(ApiResponse::DeepSeek(chat_to_deepseek_response(resp)))
        }
        (ApiResponse::OpenAIChat(resp), OutputFormat::Agnes) => {
            Ok(ApiResponse::Agnes(chat_to_agnes_response(resp)))
        }
        (ApiResponse::DeepSeek(resp), OutputFormat::OpenAIChat) => {
            Ok(ApiResponse::OpenAIChat(deepseek_to_chat_response(resp)))
        }
        (ApiResponse::DeepSeek(resp), OutputFormat::DeepSeek) => Ok(response.clone()),
        (ApiResponse::Agnes(resp), OutputFormat::OpenAIChat) => {
            Ok(ApiResponse::OpenAIChat(agnes_to_chat_response(resp)))
        }
        (ApiResponse::Agnes(resp), OutputFormat::Agnes) => Ok(response.clone()),
        _ => Err(format!(
            "Unsupported conversion: {:?} → {:?}",
            response, target_format
        )),
    }
}

/// Convert a streaming response to the target format.
pub fn convert_stream_event(
    event: &ApiStreamChunk,
    target_format: OutputFormat,
) -> Result<ApiStreamChunk, String> {
    match (event, target_format) {
        (ApiStreamChunk::Anthropic(ev), OutputFormat::OpenAIChat) => {
            let chunk = anthropic_chunk_to_openai_chunk(ev)?;
            Ok(ApiStreamChunk::OpenAIChat(chunk))
        }
        (ApiStreamChunk::OpenAIChat(ev), OutputFormat::Anthropic) => {
            let event = openai_chunk_to_anthropic_chunk(ev)?;
            Ok(ApiStreamChunk::Anthropic(event))
        }
        (ApiStreamChunk::OpenAIChat(ev), OutputFormat::DeepSeek) => {
            Ok(ApiStreamChunk::DeepSeek(openai_chunk_to_deepseek_chunk(ev)))
        }
        (ApiStreamChunk::OpenAIChat(ev), OutputFormat::Agnes) => {
            Ok(ApiStreamChunk::Agnes(openai_chunk_to_agnes_chunk(ev)))
        }
        (ApiStreamChunk::DeepSeek(ev), OutputFormat::OpenAIChat) => {
            Ok(ApiStreamChunk::OpenAIChat(deepseek_chunk_to_openai_chunk(ev)))
        }
        (ApiStreamChunk::Agnes(ev), OutputFormat::OpenAIChat) => {
            Ok(ApiStreamChunk::OpenAIChat(agnes_chunk_to_openai_chunk(ev)))
        }
        _ => Ok(event.clone()),
    }
}

/// Target output format for responses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Anthropic,
    OpenAIChat,
    OpenAIResponses,
    DeepSeek,
    Agnes,
}

impl OutputFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputFormat::Anthropic => "anthropic",
            OutputFormat::OpenAIChat => "openai_chat",
            OutputFormat::OpenAIResponses => "openai_responses",
            OutputFormat::DeepSeek => "deepseek",
            OutputFormat::Agnes => "agnes",
        }
    }
}

/// Convert OpenAI Responses API request to Chat Completions request.
pub fn responses_to_chat_completions(req: ResponsesRequest) -> Result<ChatCompletionsRequest, String> {
    let input_text = match &req.input {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(items) => {
            let mut texts = Vec::new();
            for item in items {
                if let Some(text) = item.get("content").and_then(|c| c.as_str()) {
                    texts.push(text.to_string());
                }
            }
            texts.join("\n")
        }
        _ => String::new(),
    };

    let mut messages = Vec::new();

    // Instructions become system message
    if let Some(ref instructions) = req.instructions {
        if !instructions.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: Some(serde_json::Value::String(instructions.clone())),
                ..Default::default()
            });
        }
    }

    // Input becomes user message
    if !input_text.is_empty() {
        messages.push(ChatMessage {
            role: "user".to_string(),
            content: Some(serde_json::Value::String(input_text)),
            ..Default::default()
        });
    }

    Ok(ChatCompletionsRequest {
        model: req.model,
        messages,
        max_tokens: req.max_output_tokens,
        max_completion_tokens: None,
        temperature: req.temperature,
        top_p: req.top_p,
        stream: req.stream,
        tools: req.tools.map(|tools| {
            tools.into_iter().filter_map(|t| {
                if t.tool_type != "function" {
                    return None;
                }
                Some(ChatTool {
                    tool_type: "function".to_string(),
                    function: ChatFunction {
                        name: t.name.unwrap_or_default(),
                        description: t.description,
                        parameters: t.parameters,
                        strict: None,
                    },
                })
            }).collect()
        }),
        ..Default::default()
    })
}

/// Convert Chat Completions response to OpenAI Responses API response.
pub fn chat_completions_to_responses(resp: &ChatCompletionsResponse, model: &str) -> ResponsesResponse {
    let id = format!("resp_{}", uuid::Uuid::new_v4().simple());
    let mut output_items = Vec::new();

    if let Some(choice) = resp.choices.first() {
        let text = chat_message_content_text(&choice.message.content);
        let mut content_parts = Vec::new();
        if !text.is_empty() {
            content_parts.push(ResponsesContentPart {
                part_type: "output_text".to_string(),
                text: Some(text),
                image_url: None,
            });
        }

        output_items.push(ResponsesOutput {
            output_type: "message".to_string(),
            id: Some(uuid::Uuid::new_v4().to_string()),
            role: Some("assistant".to_string()),
            content: Some(content_parts),
            status: Some("completed".to_string()),
            encrypted_content: None,
            summary: None,
            call_id: None,
            name: None,
            arguments: None,
        });
    }

    let mut status = "completed".to_string();
    let mut incomplete_details = None;
    if let Some(choice) = resp.choices.first() {
        if choice.finish_reason == "length" {
            status = "incomplete".to_string();
            incomplete_details = Some(ResponsesIncompleteDetails {
                reason: "max_output_tokens".to_string(),
            });
        }
    }

    ResponsesResponse {
        id,
        object_type: "response".to_string(),
        model: model.to_string(),
        status,
        output: output_items,
        usage: resp.usage.as_ref().map(|u| ResponsesUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            input_tokens_details: u.prompt_tokens_details.as_ref().map(|d| {
                ResponsesInputTokensDetails { cached_tokens: d.cached_tokens }
            }),
            output_tokens_details: None,
        }),
        incomplete_details,
        error: None,
    }
}

/// Extract text content from a ChatMessage content field.
fn chat_message_content_text(content: &Option<serde_json::Value>) -> String {
    match content {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(parts)) => {
            let mut texts = Vec::new();
            for part in parts {
                if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                    texts.push(text.to_string());
                }
            }
            texts.join("\n")
        }
        _ => String::new(),
    }
}
