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
