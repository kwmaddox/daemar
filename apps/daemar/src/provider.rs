//! The model seam, v0: an OpenAI-compatible chat-completions client.
//!
//! A module, not a crate, on purpose — the trait boundary is earned when a
//! second implementation exists (AGENTS.md: earned structure). Sync `ureq`:
//! one sequential call needs no runtime.

use std::fmt;

use serde::Deserialize;

/// Overall request deadline. Generous — reasoning models take their time —
/// but finite: no flight parks forever on a dead socket.
const TIMEOUT_SECS: u64 = 600;

/// The connection, not the model: one provider serves many airframes, and
/// which model flies is decided per phase by the workflow.
pub struct Provider {
    pub base_url: String,
    pub api_key: String,
}

pub struct ModelReply {
    pub text: String,
    pub prompt_tokens: u64,
    /// Cache-hit subset of prompt_tokens — billed at the cached rate.
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
    /// 200 OK but no assistant text — a response shape we refuse to guess at.
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
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    content: Option<String>,
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
    /// One turn: system + user in, assistant text and usage out.
    pub fn complete(
        &self,
        model: &str,
        system: &str,
        user: &str,
    ) -> Result<ModelReply, ProviderError> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": user },
            ],
        });

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
        let text = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .filter(|content| !content.is_empty())
            .ok_or(ProviderError::MissingContent)?;
        let usage = parsed.usage.unwrap_or_default();
        let total_tokens = if usage.total_tokens > 0 {
            usage.total_tokens
        } else {
            usage.prompt_tokens + usage.completion_tokens
        };
        Ok(ModelReply {
            text,
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
