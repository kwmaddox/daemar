//! The model seam, v0: an OpenAI-compatible chat-completions client.
//!
//! A module, not a crate, on purpose — the trait boundary is earned when a
//! second implementation exists (AGENTS.md: earned structure). Sync `ureq`:
//! one sequential call needs no runtime.

use std::fmt;

use serde::Deserialize;
use serde_json::Value;

/// Overall request deadline. Generous — reasoning models take their time —
/// but finite: no flight parks forever on a dead socket.
const TIMEOUT_SECS: u64 = 600;

/// The connection, not the model: one provider serves many airframes, and
/// which model flies is decided per phase by the workflow.
pub struct Provider {
    pub base_url: String,
    pub api_key: String,
}

/// One tool invocation the model asked for. `arguments` stays the raw JSON
/// string the provider sent — the executor parses it, and a malformed one
/// becomes an error outcome the model gets to read.
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// One turn's result: the assistant message verbatim (to echo back into the
/// conversation), whatever text and tool calls it carried, and the usage.
pub struct ChatOut {
    pub assistant: Value,
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCallRequest>,
    pub prompt_tokens: u64,
    pub cached_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

/// Every way the seam fails, as data. The moghedien taxonomy, inherited.
#[derive(Debug)]
pub enum ProviderError {
    Transport(String),
    Status {
        code: u16,
        body: String,
    },
    Decode(String),
    /// 200 OK but neither text nor tool calls — a shape we refuse to guess at.
    MissingContent,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::Transport(detail) => write!(f, "transport: {detail}"),
            ProviderError::Status { code, body } => {
                write!(f, "http {code}: {}", clip(body, 300))
            }
            ProviderError::Decode(detail) => write!(f, "bad response body: {detail}"),
            ProviderError::MissingContent => f.write_str("response carried no content"),
        }
    }
}

impl std::error::Error for ProviderError {}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: Value,
}

#[derive(Deserialize, Default, Clone, Copy)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
    #[serde(default)]
    prompt_tokens_details: PromptTokensDetails,
}

#[derive(Deserialize, Default, Clone, Copy)]
struct PromptTokensDetails {
    #[serde(default)]
    cached_tokens: u64,
}

impl Provider {
    /// One turn over an explicit message array, tools optional. The core.
    pub fn chat(
        &self,
        model: &str,
        messages: &[Value],
        tools: Option<&Value>,
    ) -> Result<ChatOut, ProviderError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let mut body = serde_json::json!({ "model": model, "messages": messages });
        if let Some(tools) = tools {
            body["tools"] = tools.clone();
            // OpenAI's chat-completions endpoint rejects function tools on
            // reasoning models unless effort is 'none' (their 400 says so:
            // "use /v1/responses or set reasoning_effort to 'none'"). The
            // Responses API is the eventual fix; this is the honest v0.
            body["reasoning_effort"] = serde_json::json!("none");
        }

        // A generation can legitimately take minutes; a hung connection must
        // not take forever. The deadline turns a dead network into a
        // Transport error — a witnessed failure — instead of a parked process.
        let response = ureq::post(&url)
            .timeout(std::time::Duration::from_secs(TIMEOUT_SECS))
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .send_json(body);

        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(code, response)) => {
                let body = response.into_string().unwrap_or_default();
                return Err(ProviderError::Status { code, body });
            }
            Err(ureq::Error::Transport(transport)) => {
                return Err(ProviderError::Transport(transport.to_string()));
            }
        };

        let parsed: ChatResponse = response
            .into_json()
            .map_err(|error| ProviderError::Decode(error.to_string()))?;
        let assistant = parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message)
            .ok_or(ProviderError::MissingContent)?;

        let text = assistant
            .get("content")
            .and_then(Value::as_str)
            .filter(|t| !t.is_empty())
            .map(str::to_string);
        let tool_calls: Vec<ToolCallRequest> = assistant
            .get("tool_calls")
            .and_then(Value::as_array)
            .map(|calls| {
                calls
                    .iter()
                    .filter_map(|call| {
                        let function = call.get("function")?;
                        Some(ToolCallRequest {
                            id: call.get("id")?.as_str()?.to_string(),
                            name: function.get("name")?.as_str()?.to_string(),
                            // Spec says arguments arrive as a JSON string,
                            // but some OpenAI-compatible providers send the
                            // object itself. Preserve either; drop neither.
                            arguments: match function.get("arguments") {
                                Some(Value::String(s)) => s.clone(),
                                Some(Value::Null) | None => "{}".to_string(),
                                Some(other) => other.to_string(),
                            },
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        if text.is_none() && tool_calls.is_empty() {
            return Err(ProviderError::MissingContent);
        }

        let usage = parsed.usage.unwrap_or_default();
        let total_tokens = if usage.total_tokens > 0 {
            usage.total_tokens
        } else {
            usage.prompt_tokens + usage.completion_tokens
        };
        Ok(ChatOut {
            assistant,
            text,
            tool_calls,
            prompt_tokens: usage.prompt_tokens,
            cached_tokens: usage.prompt_tokens_details.cached_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens,
        })
    }
}

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        let clipped: String = text.chars().take(limit).collect();
        format!("{clipped}…")
    }
}
