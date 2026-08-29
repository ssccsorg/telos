//! LLM backend abstraction and an OpenAI-compatible client.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One conversation message in the OpenAI wire shape.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: String,
    #[serde(default)]
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A tool invocation requested by the model.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// The model's reply to one completion request.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
}

/// A backend that produces completions. Injectable so the tool loop can be
/// tested without a network or a real provider.
#[async_trait::async_trait]
pub trait ChatBackend: Send + Sync {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[Value],
    ) -> Result<ChatResponse>;
}

/// OpenAI-compatible chat completions client. Deepseek is the default
/// provider; any OpenAI-shaped endpoint works.
pub struct OpenAiBackend {
    base_url: String,
    model: String,
    api_key: String,
    client: reqwest::Client,
}

impl OpenAiBackend {
    /// Configure from the environment. Returns None when no API key is set.
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("TELOS_LLM_API_KEY")
            .or_else(|_| std::env::var("LLM_API_KEY"))
            .ok()?;
        let base_url = std::env::var("TELOS_LLM_BASE_URL")
            .unwrap_or_else(|_| "https://api.deepseek.com/v1".to_string());
        let model = std::env::var("TELOS_LLM_MODEL")
            .unwrap_or_else(|_| "deepseek-chat".to_string());
        Some(Self {
            base_url,
            model,
            api_key,
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait::async_trait]
impl ChatBackend for OpenAiBackend {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[Value],
    ) -> Result<ChatResponse> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "tools": tools,
            "tool_choice": "auto",
        });
        let url = format!(
            "{}/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("llm request")?
            .error_for_status()
            .context("llm status")?;
        let data: Value = resp.json().await.context("llm json")?;
        let message = &data["choices"][0]["message"];
        let content = message["content"].as_str().map(|s| s.to_string());
        let tool_calls = message["tool_calls"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        Some(ToolCall {
                            id: tc["id"].as_str()?.to_string(),
                            name: tc["function"]["name"].as_str()?.to_string(),
                            arguments: serde_json::from_str(
                                tc["function"]["arguments"].as_str()?,
                            )
                            .ok()?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(ChatResponse {
            content,
            tool_calls,
        })
    }
}

/// Canned backend used when no API key is configured. Keeps the contract
/// flow alive without a provider.
pub struct CannedBackend;

#[async_trait::async_trait]
impl ChatBackend for CannedBackend {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[Value],
    ) -> Result<ChatResponse> {
        Ok(ChatResponse {
            content: Some(
                "Telos: no LLM configured (set TELOS_LLM_API_KEY); canned response.\n"
                    .to_string(),
            ),
            tool_calls: Vec::new(),
        })
    }
}
