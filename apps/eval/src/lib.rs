//! daemar-eval: the factory's evaluation rig.
//!
//! A deliberately invoked, paid harness that flies the REAL production
//! workflows against territories pinned to immutable Git commits, grades the
//! reports mechanically against authored answer keys, and emits comparable
//! dossiers — quality verdicts beside frozen ledger receipts.
//!
//! Doctrine, inherited from the plan on slip 019fd327:
//! - Structural parity: flights go through `factory::workflows`, never a
//!   reimplemented loop. The prompt the eval measures IS the prompt that flies.
//! - Deterministic grading: answer keys of required citations and facts,
//!   all problems at once, no LLM judge. Subjective quality is preserved as
//!   raw output for human review, never pretend-scored.
//! - The ledger receipts are the cost axis; the dossier only observes.

pub mod cases;
pub mod compare;
pub mod dossier;
pub mod grade;
pub mod pin;
pub mod run;

use std::fmt;
use std::path::PathBuf;

/// Rig-level failures: everything that stops a run before or between
/// flights. A single flight's failure is dossier DATA, not an `EvalError`.
#[derive(Debug)]
pub enum EvalError {
    /// Every corpus problem at once — a fixture author fixes one round-trip.
    Cases(Vec<cases::CaseError>),
    /// An unknown --case / --class selector; nothing was flown or paid.
    Selector(String),
    Pin(pin::PinError),
    Config(factory::config::ConfigError),
    Io {
        path: PathBuf,
        detail: String,
    },
    Dossier {
        detail: String,
    },
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::Cases(errors) => {
                writeln!(f, "the case corpus has problems:")?;
                for error in errors {
                    writeln!(f, "  {error}")?;
                }
                Ok(())
            }
            EvalError::Selector(detail) => write!(f, "{detail}"),
            EvalError::Pin(error) => write!(f, "{error}"),
            EvalError::Config(error) => write!(f, "{error}"),
            EvalError::Io { path, detail } => write!(f, "io at {}: {detail}", path.display()),
            EvalError::Dossier { detail } => write!(f, "dossier: {detail}"),
        }
    }
}

impl From<pin::PinError> for EvalError {
    fn from(error: pin::PinError) -> Self {
        EvalError::Pin(error)
    }
}

impl From<factory::config::ConfigError> for EvalError {
    fn from(error: factory::config::ConfigError) -> Self {
        EvalError::Config(error)
    }
}
