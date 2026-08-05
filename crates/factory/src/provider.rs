//! The model seam, v1: an OpenAI Responses API client.
//!
//! A module, not a crate, on purpose — the trait boundary is earned when a
//! second implementation exists (AGENTS.md: earned structure). Sync `ureq`:
//! one sequential call needs no runtime.
//!
//! Stateless by doctrine: every request sends `store:false` and resends the
//! full accumulated input — the ledger is the memory, sessions are caches,
//! and server-side stored state would put truth somewhere the fold cannot
//! reach. The price of statelessness is replay: opaque `reasoning` items
//! and `function_call` items from each turn must ride back in the next
//! turn's input, or the model's reasoning thread breaks mid-tool-loop.

use std::fmt;

use serde::Deserialize;
use serde_json::Value;

use crate::config::ReasoningEffort;

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

/// One turn's result: the output items the next turn must replay
/// (reasoning and function calls, verbatim), whatever text and tool calls
/// the turn carried, and the usage.
pub struct ResponseOut {
    /// Opaque continuation state: `reasoning` and `function_call` output
    /// items in arrival order, to be appended to the next request's input.
    pub continuation: Vec<Value>,
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
struct ResponsesBody {
    #[serde(default)]
    output: Vec<Value>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize, Default, Clone, Copy)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    total_tokens: u64,
    #[serde(default)]
    input_tokens_details: InputTokensDetails,
}

#[derive(Deserialize, Default, Clone, Copy)]
struct InputTokensDetails {
    #[serde(default)]
    cached_tokens: u64,
}

impl Provider {
    /// One turn over an explicit input array, tools optional. The core.
    /// `instructions` is the seat's system prompt; `input` is the full
    /// accumulated context, resent every turn (`store:false`).
    pub fn respond(
        &self,
        model: &str,
        instructions: &str,
        input: &[Value],
        tools: Option<&Value>,
        effort: ReasoningEffort,
    ) -> Result<ResponseOut, ProviderError> {
        let url = format!("{}/responses", self.base_url.trim_end_matches('/'));
        let mut body = serde_json::json!({
            "model": model,
            "instructions": instructions,
            "input": input,
            "reasoning": { "effort": effort.as_str() },
            "store": false,
        });
        if let Some(tools) = tools {
            body["tools"] = tools.clone();
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

        let parsed: ResponsesBody = response
            .into_json()
            .map_err(|error| ProviderError::Decode(error.to_string()))?;

        let mut continuation: Vec<Value> = Vec::new();
        let mut texts: Vec<String> = Vec::new();
        let mut tool_calls: Vec<ToolCallRequest> = Vec::new();
        for item in parsed.output {
            match item.get("type").and_then(Value::as_str) {
                // Opaque by design: replayed verbatim, never interpreted.
                Some("reasoning") => continuation.push(item),
                Some("function_call") => {
                    if let Some(call) = parse_function_call(&item) {
                        tool_calls.push(call);
                    }
                    continuation.push(item);
                }
                Some("message") => {
                    if let Some(content) = item.get("content").and_then(Value::as_array) {
                        for entry in content {
                            if entry.get("type").and_then(Value::as_str) == Some("output_text") {
                                if let Some(text) = entry
                                    .get("text")
                                    .and_then(Value::as_str)
                                    .filter(|t| !t.is_empty())
                                {
                                    texts.push(text.to_string());
                                }
                            }
                        }
                    }
                }
                // Foreign item kinds are the API's to invent; ignore.
                _ => {}
            }
        }
        let text = if texts.is_empty() {
            None
        } else {
            Some(texts.join("\n"))
        };

        if text.is_none() && tool_calls.is_empty() {
            return Err(ProviderError::MissingContent);
        }

        let usage = parsed.usage.unwrap_or_default();
        let total_tokens = if usage.total_tokens > 0 {
            usage.total_tokens
        } else {
            usage.input_tokens + usage.output_tokens
        };
        Ok(ResponseOut {
            continuation,
            text,
            tool_calls,
            prompt_tokens: usage.input_tokens,
            cached_tokens: usage.input_tokens_details.cached_tokens,
            completion_tokens: usage.output_tokens,
            total_tokens,
        })
    }
}

fn parse_function_call(item: &Value) -> Option<ToolCallRequest> {
    Some(ToolCallRequest {
        id: item.get("call_id")?.as_str()?.to_string(),
        name: item.get("name")?.as_str()?.to_string(),
        // Spec says arguments arrive as a JSON string, but some
        // OpenAI-compatible providers send the object itself. Preserve
        // either; drop neither.
        arguments: match item.get("arguments") {
            Some(Value::String(s)) => s.clone(),
            Some(Value::Null) | None => "{}".to_string(),
            Some(other) => other.to_string(),
        },
    })
}

fn clip(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        let clipped: String = text.chars().take(limit).collect();
        format!("{clipped}…")
    }
}
