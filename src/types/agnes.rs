//! Agnes API type definitions.
//!
//! Agnes uses an API format similar to OpenAI but with Agnes-specific fields
//! and conventions. This module provides type definitions for translating
//! between Agnes format and OpenAI/Anthropic formats.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Agnes chat request - compatible with OpenAI format with extras.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgnesChatRequest {
    pub model: String,
    pub messages: Vec<AgnesMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AgnesTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    /// Agnes-specific: custom metadata passed through to upstream.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    /// Agnes-specific: agent_id for multi-agent routing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_body: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgnesMessage {
    pub role: String,
    pub content: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgnesTool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: AgnesFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgnesFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgnesChatResponse {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<AgnesChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<AgnesUsage>,
    /// Agnes-specific: response metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agnes_meta: Option<AgnesMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgnesChoice {
    pub index: usize,
    pub message: AgnesMessage,
    #[serde(rename = "finish_reason")]
    pub finish_reason: String,
    /// Streaming only: delta content in a chunk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<AgnesDelta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgnesUsage {
    #[serde(rename = "prompt_tokens")]
    pub prompt_tokens: i64,
    #[serde(rename = "completion_tokens")]
    pub completion_tokens: i64,
    #[serde(rename = "total_tokens")]
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgnesMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub routing_info: Option<Value>,
}

/// Agnes streaming chunk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgnesChatChunk {
    pub id: String,
    #[serde(rename = "object")]
    pub object_type: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<AgnesChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<AgnesUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgnesChunkChoice {
    pub index: usize,
    pub delta: AgnesDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "finish_reason")]
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgnesDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}
