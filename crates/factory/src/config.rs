//! Config: env is the serde edge of a CLI, parsed once, here.

use std::fmt;
use std::process::ExitCode;

use crate::provider::Provider;
use crate::registry::{self, Registry};
use crate::roster::{self, Role};

pub fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

pub fn engineer() -> String {
    env("USER").unwrap_or_else(|| "engineer".to_string())
}

pub fn ledgers_dir() -> String {
    env("DAEMAR_LEDGERS").unwrap_or_else(|| "ledgers".to_string())
}

/// The default territory: wherever the engineer is standing.
pub fn cwd_territory() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.display().to_string())
        .unwrap_or_default()
}

#[derive(Debug)]
pub enum ConfigError {
    Missing(String),
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

pub struct Config {
    pub provider: Provider,
    pub ledgers: String,
    pub airframes: String,
    pub engineer: String,
    /// Model per role, resolved fail-fast at startup from the roster's env
    /// bindings (each falls back to DAEMAR_MODEL). Indexed by Role::ALL order.
    models: [String; Role::ALL.len()],
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let default_model = env("DAEMAR_MODEL");
        let mut models: [String; Role::ALL.len()] = Default::default();
        for (slot, role) in Role::ALL.iter().enumerate() {
            let agent = roster::agent(*role);
            models[slot] = env(agent.model_env)
                .or_else(|| default_model.clone())
                .ok_or_else(|| {
                    ConfigError::Missing(format!("{} (or DAEMAR_MODEL)", agent.model_env))
                })?;
        }
        Ok(Config {
            provider: Provider {
                base_url: env("DAEMAR_BASE_URL")
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                api_key: env("OPENAI_API_KEY")
                    .ok_or_else(|| ConfigError::Missing("OPENAI_API_KEY".to_string()))?,
            },
            ledgers: ledgers_dir(),
            airframes: env("DAEMAR_AIRFRAMES").unwrap_or_else(|| "airframes.toml".to_string()),
            engineer: engineer(),
            models,
        })
    }

    /// The airframe bound to a role. The binding is env-driven today; when
    /// the roster becomes data, this is the seam that changes.
    pub fn model_for(&self, role: Role) -> &str {
        let slot = Role::ALL
            .iter()
            .position(|r| *r == role)
            .expect("Role::ALL covers every role");
        &self.models[slot]
    }
}

pub fn with_config<F>(flight: F) -> ExitCode
where
    F: FnOnce(&Config) -> Result<bool, ledger::LedgerError>,
{
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("daemar: {error}");
            return ExitCode::from(2);
        }
    };
    match flight(&config) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            // A ledger that cannot be written is a flight that cannot be
            // recorded: nothing to salvage, fail loud.
            eprintln!("daemar: ledger failure: {error}");
            ExitCode::FAILURE
        }
    }
}

// ── Pricing (resolved per airframe; not-priced is never silent) ──────────────

pub enum Pricing {
    Priced(registry::Price),
    Unregistered { model: String },
    RegistryBroken { detail: String },
}

impl Pricing {
    pub fn resolve(airframes: &str, model: &str) -> Pricing {
        match Registry::load(airframes.as_ref()) {
            Ok(registry) => match registry.price(model) {
                Some(price) => Pricing::Priced(price),
                None => Pricing::Unregistered {
                    model: model.to_string(),
                },
            },
            Err(error) => Pricing::RegistryBroken {
                detail: error.to_string(),
            },
        }
    }

    pub fn cost(&self, prompt_tokens: u64, cached_tokens: u64, completion_tokens: u64) -> f64 {
        match self {
            Pricing::Priced(price) => price.cost(prompt_tokens, cached_tokens, completion_tokens),
            Pricing::Unregistered { .. } | Pricing::RegistryBroken { .. } => 0.0,
        }
    }

    pub fn complaint(&self) -> Option<String> {
        match self {
            Pricing::Priced(_) => None,
            Pricing::Unregistered { model } => Some(format!(
                "airframe {model} is not in airframes.toml; cost unrecorded"
            )),
            Pricing::RegistryBroken { detail } => Some(format!(
                "airframe registry unreadable; cost unrecorded — {detail}"
            )),
        }
    }
}
