//! Config: env is the serde edge, parsed once, here. `DAEMAR_HOME` roots the
//! tower's state so any interface — CLI, MCP client, someday a daemon — finds
//! the same factory regardless of where the process was spawned.

use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use crate::provider::Provider;
use crate::registry::{self, Registry};
use crate::roster::{self, Role};

pub fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

pub fn engineer() -> String {
    env("USER").unwrap_or_else(|| "engineer".to_string())
}

/// The tower's root: where ledgers, airframes, and secrets live when the
/// process wasn't launched from the repo itself.
pub fn home() -> Option<PathBuf> {
    env("DAEMAR_HOME").map(PathBuf::from)
}

/// A relative default resolved against DAEMAR_HOME when set; the bare
/// default (cwd-relative, the original CLI behavior) otherwise.
fn homed_default(relative: &str) -> String {
    match home() {
        Some(root) => root.join(relative).display().to_string(),
        None => relative.to_string(),
    }
}

pub fn ledgers_dir() -> String {
    env("DAEMAR_LEDGERS").unwrap_or_else(|| homed_default("ledgers"))
}

fn airframes_path() -> String {
    env("DAEMAR_AIRFRAMES").unwrap_or_else(|| homed_default("airframes.toml"))
}

#[derive(Debug)]
pub enum ConfigError {
    Missing(String),
    /// The secrets file existed but could not be decrypted or parsed.
    Secrets(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing(name) => write!(
                f,
                "{name} is not set — export it, or set DAEMAR_HOME so the \
                 tower can decrypt its own secrets"
            ),
            ConfigError::Secrets(detail) => write!(f, "{detail}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// The tower fetches its own key: process env first; on a miss, decrypt
/// `$DAEMAR_HOME/secrets/daemar.enc.env` with sops — once, lazily, into this
/// process's memory. The key never rides in any exec environment, so there
/// is nothing for `ps eww` or `/proc/<pid>/environ` to find.
struct Vault {
    /// None until the first env miss forces a load. The loaded map is empty
    /// when there is no home or no secrets file — that is not an error;
    /// missing values fail loud at their own lookups.
    secrets: Option<HashMap<String, String>>,
}

impl Vault {
    fn new() -> Self {
        Vault { secrets: None }
    }

    fn get(&mut self, name: &str) -> Result<Option<String>, ConfigError> {
        if let Some(value) = env(name) {
            return Ok(Some(value));
        }
        if self.secrets.is_none() {
            self.secrets = Some(decrypt_secrets()?);
        }
        Ok(self
            .secrets
            .as_ref()
            .expect("loaded above")
            .get(name)
            .cloned())
    }
}

fn decrypt_secrets() -> Result<HashMap<String, String>, ConfigError> {
    let Some(root) = home() else {
        return Ok(HashMap::new());
    };
    let file = root.join("secrets/daemar.enc.env");
    if !file.exists() {
        return Ok(HashMap::new());
    }
    let mut sops = std::process::Command::new("sops");
    sops.arg("-d").arg(&file);
    // The house key location is XDG (~/.config/sops/age); macOS sops defaults
    // to ~/Library/Application Support instead, so point it home unless the
    // environment already chose.
    if env("SOPS_AGE_KEY_FILE").is_none() {
        if let Some(key) = env("HOME")
            .map(|h| PathBuf::from(h).join(".config/sops/age/keys.txt"))
            .filter(|k| k.exists())
        {
            sops.env("SOPS_AGE_KEY_FILE", key);
        }
    }
    let out = sops
        .output()
        .map_err(|e| ConfigError::Secrets(format!("could not run sops: {e}")))?;
    if !out.status.success() {
        return Err(ConfigError::Secrets(format!(
            "sops -d {} failed: {}",
            file.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Tolerate shell-flavored dotenv: an `export ` prefix and matched
        // surrounding quotes both belong to the shell, not the value.
        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            map.insert(key.trim().to_string(), value.to_string());
        }
    }
    Ok(map)
}

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
        let mut vault = Vault::new();
        let default_model = vault.get("DAEMAR_MODEL")?;
        let mut models: [String; Role::ALL.len()] = Default::default();
        for (slot, role) in Role::ALL.iter().enumerate() {
            let agent = roster::agent(*role);
            models[slot] = vault
                .get(agent.model_env)?
                .or_else(|| default_model.clone())
                .ok_or_else(|| {
                    ConfigError::Missing(format!("{} (or DAEMAR_MODEL)", agent.model_env))
                })?;
        }
        Ok(Config {
            provider: Provider {
                base_url: vault
                    .get("DAEMAR_BASE_URL")?
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
                api_key: vault
                    .get("OPENAI_API_KEY")?
                    .ok_or_else(|| ConfigError::Missing("OPENAI_API_KEY".to_string()))?,
            },
            ledgers: ledgers_dir(),
            airframes: airframes_path(),
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
