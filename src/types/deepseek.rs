//! DeepSeek API type definitions.
//!
//! DeepSeek uses an API format compatible with OpenAI, with some extensions:
//! - base_url: https://api.deepseek.com
//! - Anthropic compat base_url: https://api.deepseek.com/anthropic
//! - Extra fields: `thinking`, `reasoning_effort`
//! - Models: deepseek-v4-flash, deepseek-v4-pro, deepseek-chat, deepseek-reasoner

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// DeepSeek extends ChatCompletions with thinking/reasoning fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekChatRequest {
    pub model: String,
    pub messages: Vec<DeepSeekMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<DeepSeekThinking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<DeepSeekTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeepSeekMessage {
    pub role: String,
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Thinking mode configuration unique to DeepSeek.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekThinking {
    #[serde(rename = "type")]
    pub thinking_type: String, // "enabled", "disabled"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekTool {
    #[serde(rename = "type")]
    pub tool_type: String, // "function"
    pub function: DeepSeekFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
}

/// DeepSeek response includes reasoning_content field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekChatResponse {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<DeepSeekChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<DeepSeekUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekChoice {
    pub index: usize,
    pub message: DeepSeekMessage,
    #[serde(rename = "finish_reason")]
    pub finish_reason: String,
    /// Streaming only: delta content in a chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<DeepSeekDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekUsage {
    #[serde(rename = "prompt_tokens")]
    pub prompt_tokens: i64,
    #[serde(rename = "completion_tokens")]
    pub completion_tokens: i64,
    #[serde(rename = "total_tokens")]
    pub total_tokens: i64,
    #[serde(rename = "prompt_tokens_details", skip_serializing_if = "Option::is_none")]
    pub prompt_details: Option<DeepSeekPromptDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekPromptDetails {
    #[serde(rename = "cached_tokens")]
    pub cached_tokens: i64,
}

/// DeepSeek streaming chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekChatChunk {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<DeepSeekChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<DeepSeekUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekChunkChoice {
    pub index: usize,
    pub delta: DeepSeekDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "finish_reason")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeepSeekDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

// DeepSeek model registry
pub const DEEPSEEK_MODELS: &[&str] = &[
    "deepseek-v4-flash",
    "deepseek-v4-pro",
    "deepseek-chat",
    "deepseek-reasoner",
];

/// Map DeepSeek model aliases to canonical names.
pub fn normalize_deepseek_model(model: &str) -> &str {
    match model {
        "deepseek-chat" => "deepseek-v4-flash",
        "deepseek-reasoner" => "deepseek-v4-flash",
        m => m,
    }
}
