//! API format type definitions for Anthropic Messages, OpenAI Chat Completions,
//! OpenAI Responses, DeepSeek, and Agnes platforms.
//!
//! This module provides the core type system that enables bidirectional
//! conversion between multiple LLM API formats, allowing the gateway to
//! accept requests in any format and forward them to upstream providers
//! in the format they expect.

pub mod anthropic;
pub mod openai;
pub mod deepseek;
pub mod agnes;
