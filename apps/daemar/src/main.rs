//! The factory's runner. v0: the first loop.
//!
//!     daemar "<request>"
//!
//! One slip, one phase, one model call, no tools, no checks — the board and
//! the controller's eyes are the check (CONTEXT.md build order). The loop is
//! shaped as a turn loop that happens to run exactly one turn, so tools are
//! a later addition, not surgery.
//!
//! Every event is appended and flushed as it happens; the board's doorbell
//! narrates the flight live. Errors close the slip honestly (rejected, with
//! the reason on the ledger). A crash leaves no terminator and the board
//! derives interrupted — by design.

use std::fmt;
use std::process::ExitCode;

use ledger::{Kind, Lane, LedgerWriter, PhaseOutcome, SlipId, SlipOutcome};

mod provider;
mod registry;

use provider::Provider;
use registry::{Price, Registry};

const WORKFLOW: &str = "prompt";
const PHASE: &str = "respond";
const OWNER: &str = "responder";
const SYSTEM_PROMPT: &str = "You are the first agent of daemar, a software factory. \
Answer the request directly and completely, in plain text.";

// ── Config (env is the serde edge of a CLI; parsed once, here) ───────────────

struct Config {
    provider: Provider,
    ledgers: String,
    airframes: String,
    engineer: String,
}

/// What the registry had to say about this flight's airframe, resolved once
/// before takeoff. Not-priced is never silent: it becomes a note on the
/// ledger, so the audit records WHY a receipt reads zero.
enum Pricing {
    Priced(Price),
    Unregistered { model: String },
    RegistryBroken { detail: String },
}

impl Pricing {
    fn resolve(config: &Config) -> Pricing {
        match Registry::load(config.airframes.as_ref()) {
            Ok(registry) => match registry.price(&config.provider.model) {
                Some(price) => Pricing::Priced(price),
                None => Pricing::Unregistered { model: config.provider.model.clone() },
            },
            Err(error) => Pricing::RegistryBroken { detail: error.to_string() },
        }
    }

    fn cost(&self, prompt_tokens: u64, cached_tokens: u64, completion_tokens: u64) -> f64 {
        match self {
            Pricing::Priced(price) => price.cost(prompt_tokens, cached_tokens, completion_tokens),
            Pricing::Unregistered { .. } | Pricing::RegistryBroken { .. } => 0.0,
        }
    }

    fn complaint(&self) -> Option<String> {
        match self {
            Pricing::Priced(_) => None,
            Pricing::Unregistered { model } => Some(format!(
                "airframe {model} is not in airframes.toml; cost unrecorded"
            )),
            Pricing::RegistryBroken { detail } => {
                Some(format!("airframe registry unreadable; cost unrecorded — {detail}"))
            }
        }
    }
}

#[derive(Debug)]
enum ConfigError {
    Missing(&'static str),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(name) => write!(
                f,
                "{name} is not set — cd into the repo so direnv decrypts secrets, \
                 or export it"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        let env = |name: &'static str| std::env::var(name).ok().filter(|v| !v.is_empty());
        Ok(Config {
            provider: Provider {
                base_url: env("DAEMAR_BASE_URL")
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                api_key: env("OPENAI_API_KEY").ok_or(ConfigError::Missing("OPENAI_API_KEY"))?,
                model: env("DAEMAR_MODEL").ok_or(ConfigError::Missing("DAEMAR_MODEL"))?,
            },
            ledgers: env("DAEMAR_LEDGERS").unwrap_or_else(|| "ledgers".to_string()),
            airframes: env("DAEMAR_AIRFRAMES").unwrap_or_else(|| "airframes.toml".to_string()),
            engineer: env("USER").unwrap_or_else(|| "engineer".to_string()),
        })
    }
}

// ── The flight ───────────────────────────────────────────────────────────────

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let request = if args.len() == 1 && args[0] == "-" {
        // The request from stdin: `git diff | daemar -`. The pipe is how real
        // repo content reaches a toolless loop.
        let mut buffer = String::new();
        use std::io::Read;
        if let Err(error) = std::io::stdin().read_to_string(&mut buffer) {
            eprintln!("daemar: reading stdin: {error}");
            return ExitCode::from(2);
        }
        buffer
    } else {
        args.join(" ")
    };
    if request.trim().is_empty() {
        eprintln!("usage: daemar \"<request>\"   or   ... | daemar -");
        return ExitCode::from(2);
    }
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("daemar: {error}");
            return ExitCode::from(2);
        }
    };
    let pricing = Pricing::resolve(&config);
    if let Some(complaint) = pricing.complaint() {
        eprintln!("daemar: {complaint}");
    }
    match fly(&config, &request, &pricing) {
        Ok(accepted) => {
            if accepted {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            // A ledger that cannot be written is a flight that cannot be
            // recorded: nothing to salvage, fail loud.
            eprintln!("daemar: ledger failure: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Run the whole flight, appending every event. Returns whether the slip
/// closed accepted. Only a ledger-write failure aborts the recording itself.
fn fly(config: &Config, request: &str, pricing: &Pricing) -> Result<bool, ledger::LedgerError> {
    let slip_id = SlipId(uuid::Uuid::now_v7().to_string());
    let mut w = LedgerWriter::create(config.ledgers.as_ref(), slip_id)?;

    w.append(&Kind::SlipOpened {
        request: request.to_string(),
        workflow: WORKFLOW.to_string(),
        engineer: config.engineer.clone(),
    })?;
    w.append(&Kind::PhaseStarted {
        phase: PHASE.to_string(),
        owner: OWNER.to_string(),
        lane: Lane::Agent,
    })?;
    w.append(&Kind::ModelRequested {
        phase: PHASE.to_string(),
        model: config.provider.model.clone(),
        system: SYSTEM_PROMPT.to_string(),
        user: request.to_string(),
    })?;

    match config.provider.complete(SYSTEM_PROMPT, request) {
        Ok(reply) => {
            let cost = pricing.cost(reply.prompt_tokens, reply.cached_tokens, reply.completion_tokens);
            w.append(&Kind::ModelCall {
                phase: PHASE.to_string(),
                model: config.provider.model.clone(),
                tokens: reply.total_tokens,
                prompt_tokens: reply.prompt_tokens,
                cached_tokens: reply.cached_tokens,
                completion_tokens: reply.completion_tokens,
                cost,
            })?;
            if let Some(complaint) = pricing.complaint() {
                w.append(&Kind::Note { text: complaint })?;
            }
            w.append(&Kind::SectionWritten {
                section: "response.v1".to_string(),
                by: OWNER.to_string(),
                summary: summarize(&reply.text),
                body: reply.text.clone(),
            })?;
            w.append(&Kind::PhaseEnded {
                phase: PHASE.to_string(),
                outcome: PhaseOutcome::Success,
            })?;
            w.append(&Kind::SlipClosed {
                outcome: SlipOutcome::Accepted,
                reason: String::new(),
            })?;
            println!("{}", reply.text.trim_end());
            eprintln!(
                "\nslip {} · accepted · {} tokens · ${cost:.4} · board: /slip/{}",
                w.slip_id(),
                reply.total_tokens,
                w.slip_id()
            );
            Ok(true)
        }
        Err(error) => {
            let reason = error.to_string();
            w.append(&Kind::PhaseEnded {
                phase: PHASE.to_string(),
                outcome: PhaseOutcome::Error,
            })?;
            w.append(&Kind::SlipClosed { outcome: SlipOutcome::Rejected, reason: reason.clone() })?;
            eprintln!("daemar: model call failed: {reason}");
            eprintln!("slip {} · rejected · board: /slip/{}", w.slip_id(), w.slip_id());
            Ok(false)
        }
    }
}

/// The strip's table line: first line of the response, clipped.
fn summarize(text: &str) -> String {
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if first.chars().count() <= 110 {
        first.to_string()
    } else {
        let clipped: String = first.chars().take(109).collect();
        format!("{clipped}…")
    }
}
