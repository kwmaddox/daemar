//! The airframe registry: the factory's model catalog, pricing column first.
//!
//! Cost is computed here at flight time and frozen onto the ledger as a
//! receipt. Editing the registry never rewrites history. A model with no
//! entry prices at nothing — the caller is responsible for saying so, loudly,
//! on the ledger.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct Registry {
    #[serde(default)]
    models: HashMap<String, Price>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Price {
    /// USD per 1M input tokens.
    pub input: f64,
    /// USD per 1M output tokens.
    pub output: f64,
    /// USD per 1M cache-hit input tokens. Absent means cached tokens price
    /// at the full input rate — conservative, never a hidden discount.
    #[serde(default)]
    pub cached_input: Option<f64>,
}

#[derive(Debug)]
pub enum RegistryError {
    Io { path: String, detail: String },
    Parse { path: String, detail: String },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegistryError::Io { path, detail } => write!(f, "registry io at {path}: {detail}"),
            RegistryError::Parse { path, detail } => write!(f, "registry parse at {path}: {detail}"),
        }
    }
}

impl std::error::Error for RegistryError {}

impl Registry {
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        let text = std::fs::read_to_string(path).map_err(|e| RegistryError::Io {
            path: path.display().to_string(),
            detail: e.to_string(),
        })?;
        toml::from_str(&text).map_err(|e| RegistryError::Parse {
            path: path.display().to_string(),
            detail: e.to_string(),
        })
    }

    pub fn price(&self, model: &str) -> Option<Price> {
        self.models.get(model).copied()
    }
}

impl Price {
    /// The receipt: three line items per million. `cached_tokens` is the
    /// cache-hit subset of `prompt_tokens`.
    pub fn cost(&self, prompt_tokens: u64, cached_tokens: u64, completion_tokens: u64) -> f64 {
        let cached = cached_tokens.min(prompt_tokens);
        let uncached = prompt_tokens - cached;
        let cached_rate = self.cached_input.unwrap_or(self.input);
        (uncached as f64 * self.input
            + cached as f64 * cached_rate
            + completion_tokens as f64 * self.output)
            / 1_000_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_parses_and_prices_a_receipt() {
        let registry: Registry = toml::from_str(
            r#"
            [models."gpt-5.6-terra"]
            input = 2.0
            cached_input = 0.2
            output = 8.0
            updated = "2026-08-04"
            "#,
        )
        .expect("parses");
        let price = registry.price("gpt-5.6-terra").expect("listed");
        // 60k uncached at $2/M = $0.12; 40k cached at $0.2/M = $0.008;
        // 50k out at $8/M = $0.40.
        assert!((price.cost(100_000, 40_000, 50_000) - 0.528).abs() < 1e-9);
        assert!(registry.price("unlisted-model").is_none());
    }

    #[test]
    fn missing_cached_rate_prices_cache_hits_at_full_input() {
        let registry: Registry =
            toml::from_str("[models.m]\ninput = 1.0\noutput = 2.0\n").expect("parses");
        let price = registry.price("m").expect("listed");
        // All 100k prompt tokens at $1/M regardless of the cached split.
        assert!((price.cost(100_000, 90_000, 0) - 0.10).abs() < 1e-9);
    }

    #[test]
    fn unknown_fields_like_updated_are_tolerated() {
        // `updated` is for humans; the parser must not choke on annotations.
        let registry: Registry =
            toml::from_str("[models.m]\ninput = 1.0\noutput = 2.0\nupdated = \"2026-01-01\"\n")
                .expect("parses");
        assert!(registry.price("m").is_some());
    }
}
