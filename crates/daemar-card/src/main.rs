//! `card` — the agent-facing CLI for Daemar Cards (PER-82).
//!
//! A thin client over the `daemar_card` core: flag parsing, database
//! resolution, and JSON rendering live here; every domain rule lives in
//! the library. Machine-facing contract (S1-B12): success is JSON on
//! stdout with assigned IDs and sequences; failure is JSON on stderr with
//! a category (`validation | conflict | missing | storage`) and a
//! non-zero exit.

use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use clap::{Arg, ArgMatches, Command};
use serde_json::{json, Value};
use time::format_description::well_known::Rfc3339;

use daemar_card::{
    AppendEntry, CardId, CreateCard, EntryType, Error, Payload, Producer, ProducerKind, Store,
    CURRENT_SCHEMA_VERSION,
};

/// Environment variable selecting the database (S1-B14).
const DB_ENV_VAR: &str = "DAEMAR_DB";
/// The factory's home directory under the operator's home: daemar is a
/// durable service with one standard environment; producers wander
/// through worktrees, the factory does not.
const FACTORY_HOME: &str = ".daemar";
/// Database filename inside the factory home.
const DB_FILE: &str = "daemar.db";
/// Optional dotenv file inside the factory home — read from the factory
/// home only, never from the caller's working directory, and parsed
/// without touching the process environment (only `DAEMAR_DB` is
/// honored from it).
const ENV_FILE: &str = ".env";

/// Resolved runtime configuration. Precedence: `--db` flag > process env
/// > factory-home `.env` > factory-home default (milestone Q8).
struct Config {
    db_path: PathBuf,
    source: ConfigSource,
}

/// Where the active database path came from; reported by `card db-path`
/// so "which database am I talking to" is always one command away.
enum ConfigSource {
    Flag,
    Env,
    DotEnv,
    Default,
}

impl ConfigSource {
    fn name(&self) -> &'static str {
        match self {
            ConfigSource::Flag => "flag",
            ConfigSource::Env => "env",
            ConfigSource::DotEnv => "dotenv",
            ConfigSource::Default => "default",
        }
    }
}

/// A CLI failure ready for rendering: a domain error, an invalid
/// invocation, or a configuration problem the library never sees.
/// `FactoryHome` keeps its typed io source (C3) instead of formatting it
/// into prose at construction.
enum Failure {
    Domain(Error),
    Invalid(String),
    Config(&'static str),
    FactoryHome {
        path: PathBuf,
        source: std::io::Error,
    },
    DotEnvFile {
        path: PathBuf,
        source: dotenvy::Error,
    },
}

impl Failure {
    fn to_json(&self) -> Value {
        let (category, message) = match self {
            Failure::Domain(error) => (error.category().to_string(), error.to_string()),
            Failure::Invalid(message) => ("validation".to_owned(), message.clone()),
            Failure::Config(message) => ("storage".to_owned(), (*message).to_owned()),
            Failure::FactoryHome { path, source } => (
                "storage".to_owned(),
                format!(
                    "cannot prepare the factory home {}: {source}",
                    path.display()
                ),
            ),
            Failure::DotEnvFile { path, source } => (
                "storage".to_owned(),
                format!(
                    "cannot load the factory dotenv {}: {source}",
                    path.display()
                ),
            ),
        };
        json!({ "error": { "category": category, "message": message } })
    }
}

impl From<Error> for Failure {
    fn from(error: Error) -> Failure {
        Failure::Domain(error)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    // Clap must not exit with prose: syntax failures belong to the same
    // JSON contract as every other failure (S1-B12, deep-review finding
    // 3). Help and version remain human-facing text on stdout.
    let matches = match cli().try_get_matches() {
        Ok(matches) => matches,
        Err(parse_error) => return render_parse_outcome(&parse_error),
    };
    match run(matches).await {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(failure) => {
            eprintln!("{}", failure.to_json());
            ExitCode::FAILURE
        }
    }
}

fn render_parse_outcome(parse_error: &clap::Error) -> ExitCode {
    let human_facing = matches!(
        parse_error.kind(),
        clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
    );
    if human_facing {
        match parse_error.print() {
            Ok(()) => ExitCode::SUCCESS,
            Err(_unwritable_stdout) => ExitCode::FAILURE,
        }
    } else {
        let failure = Failure::Invalid(parse_error.to_string());
        eprintln!("{}", failure.to_json());
        ExitCode::FAILURE
    }
}

async fn run(matches: clap::ArgMatches) -> Result<Value, Failure> {
    let config = resolve_config(matches.get_one::<PathBuf>("db").cloned())?;
    match matches.subcommand() {
        // ast-grep-ignore: no-string-literal-dispatch -- parse boundary: clap subcommand names, converted once
        Some(("db-path", _)) => Ok(json!({
            "db_path": config.db_path,
            "source": config.source.name(),
        })),
        // ast-grep-ignore: no-string-literal-dispatch -- parse boundary: clap subcommand names, converted once
        Some(("create", sub)) => create(sub, &config).await,
        // ast-grep-ignore: no-string-literal-dispatch -- parse boundary: clap subcommand names, converted once
        Some(("append", sub)) => append(sub, &config).await,
        // ast-grep-ignore: no-string-literal-dispatch -- parse boundary: clap subcommand names, converted once
        Some(("history", sub)) => history(sub, &config).await,
        // ast-grep-ignore: no-string-literal-dispatch -- parse boundary: clap subcommand names, converted once
        Some(("list", _)) => list(&config).await,
        _ => Err(Failure::Config("unknown command")),
    }
}

fn cli() -> Command {
    let producer_args = [
        Arg::new("producer")
            .long("producer")
            .value_name("ID")
            .help("Producer identity recorded on the entry (e.g. claude, codex)"),
        Arg::new("producer-kind")
            .long("producer-kind")
            .value_name("KIND")
            .help("Producer kind: agent, operator, or factory"),
        Arg::new("idempotency-key")
            .long("idempotency-key")
            .value_name("KEY")
            .help("Retry token: replaying the same request returns the original result"),
    ];
    Command::new("card")
        .about("Daemar Cards: append-only workflow records for factory tasks")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .disable_help_subcommand(true)
        .arg(
            Arg::new("db")
                .long("db")
                .global(true)
                .value_name("PATH")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Database path (takes precedence over DAEMAR_DB and the factory default)"),
        )
        .subcommand(
            Command::new("create")
                .about("Open a Card for a task")
                .arg(
                    Arg::new("title")
                        .long("title")
                        .required(true)
                        .value_name("TEXT"),
                )
                .arg(Arg::new("task-key").long("task-key").value_name("KEY"))
                .arg(Arg::new("workspace").long("workspace").value_name("REF"))
                .args(producer_args.clone()),
        )
        .subcommand(
            Command::new("append")
                .about("Append a typed entry to a Card")
                .arg(Arg::new("card-id").required(true).value_name("CARD_ID"))
                .arg(
                    Arg::new("entry-type")
                        .long("entry-type")
                        .required(true)
                        .value_name("TYPE"),
                )
                .arg(Arg::new("payload").long("payload").value_name("JSON"))
                .arg(Arg::new("stage").long("stage").value_name("TEXT"))
                .arg(Arg::new("summary").long("summary").value_name("TEXT"))
                .arg(
                    Arg::new("schema-version")
                        .long("schema-version")
                        .value_name("N"),
                )
                .args(producer_args),
        )
        .subcommand(
            Command::new("history")
                .about("Read a Card's ordered entries")
                .arg(Arg::new("card-id").required(true).value_name("CARD_ID"))
                .arg(Arg::new("entry-type").long("entry-type").value_name("TYPE")),
        )
        .subcommand(Command::new("list").about("List Cards in creation order"))
        .subcommand(
            Command::new("db-path").about("Report the active database path and where it came from"),
        )
}

fn resolve_config(flag: Option<PathBuf>) -> Result<Config, Failure> {
    if let Some(db_path) = flag {
        return Ok(Config {
            db_path,
            source: ConfigSource::Flag,
        });
    }
    if let Some(db_path) = std::env::var_os(DB_ENV_VAR) {
        return Ok(Config {
            db_path: PathBuf::from(db_path),
            source: ConfigSource::Env,
        });
    }
    let home = std::env::var_os("HOME").ok_or(Failure::Config(
        "cannot resolve the factory home: HOME is not set",
    ))?;
    let factory_home = PathBuf::from(home).join(FACTORY_HOME);
    if let Some(db_path) = dotenv_db_path(&factory_home)? {
        return Ok(Config {
            db_path,
            source: ConfigSource::DotEnv,
        });
    }
    Ok(Config {
        db_path: factory_home.join(DB_FILE),
        source: ConfigSource::Default,
    })
}

/// Reads `DAEMAR_DB` from the factory-home dotenv **without mutating the
/// process environment**: Tokio's worker threads are already running, and
/// `std::env::set_var` is unsound in a multithreaded program (deep-review
/// finding: dotenv env mutation). The file is parsed in full so a
/// present-but-broken dotenv fails loudly — silent fallback would split
/// the durable record across two stores; only genuine absence is
/// ignorable. Duplicate keys keep `dotenvy::from_path`'s documented
/// first-declaration-wins convention, so a stray later line cannot
/// silently redirect an existing installation's record.
fn dotenv_db_path(factory_home: &std::path::Path) -> Result<Option<PathBuf>, Failure> {
    let env_file = factory_home.join(ENV_FILE);
    let entries = match dotenvy::from_path_iter(&env_file) {
        Ok(entries) => entries,
        Err(error) if error.not_found() => return Ok(None),
        Err(source) => {
            return Err(Failure::DotEnvFile {
                path: env_file,
                source,
            })
        }
    };
    let mut selected = None;
    for entry in entries {
        let (key, value) = entry.map_err(|source| Failure::DotEnvFile {
            path: env_file.clone(),
            source,
        })?;
        // First declaration wins; the loop still consumes every entry so
        // a parse error after the match stays loud.
        if key == DB_ENV_VAR && selected.is_none() {
            selected = Some(PathBuf::from(value));
        }
    }
    Ok(selected)
}

/// Owner-only mode for the factory home: the durable Card log must not
/// be readable by unrelated local OS users (deep-review finding 4).
#[cfg(unix)]
const FACTORY_HOME_MODE: u32 = 0o700;

async fn open_store(config: &Config) -> Result<Store, Failure> {
    // Only the Default path lives in the factory home. A dotenv-selected
    // database is operator-selected exactly like Env/Flag: its parent
    // directory is not the factory's to create or tighten (deep-review
    // finding 2); the database files themselves are tightened in the
    // store regardless of location.
    if matches!(config.source, ConfigSource::Default) {
        if let Some(parent) = config.db_path.parent() {
            prepare_factory_home(parent).map_err(|source| Failure::FactoryHome {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }
    Ok(Store::open(&config.db_path).await?)
}

/// Creates the factory home if absent and pins it owner-only, whatever
/// the umask. Explicit `--db`/`DAEMAR_DB` locations are operator-chosen;
/// their parent directories belong to the operator, so only the database
/// files themselves (handled in the store) are tightened there.
fn prepare_factory_home(parent: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(FACTORY_HOME_MODE))?;
    }
    Ok(())
}

/// Clap enforces required arguments; this refuses explicitly instead of
/// fabricating an empty value if that enforcement ever changes.
fn require_arg(sub: &ArgMatches, name: &str) -> Result<String, Failure> {
    sub.get_one::<String>(name)
        .cloned()
        .ok_or_else(|| Failure::Invalid(format!("{name} is required")))
}

fn require_producer(sub: &ArgMatches) -> Result<Producer, Failure> {
    let id = sub.get_one::<String>("producer");
    let kind = sub.get_one::<String>("producer-kind");
    match (id, kind) {
        (Some(id), Some(kind)) => Ok(Producer {
            id: id.clone(),
            kind: ProducerKind::from_str(kind)?,
        }),
        (None, _) | (_, None) => Err(Failure::Domain(Error::MissingProducer)),
    }
}

async fn create(sub: &ArgMatches, config: &Config) -> Result<Value, Failure> {
    let producer = require_producer(sub)?;
    let store = open_store(config).await?;
    let card_id = store
        .create_card(CreateCard {
            title: require_arg(sub, "title")?,
            task_key: sub.get_one::<String>("task-key").cloned(),
            workspace: sub.get_one::<String>("workspace").cloned(),
            producer,
            idempotency_key: sub.get_one::<String>("idempotency-key").cloned(),
        })
        .await?;
    Ok(json!({ "card_id": card_id.to_string() }))
}

/// Extract payload from CLI arguments based on entry type (C16).
/// Enforces per-type argument policies and delegates to domain constructors.
fn payload_from_args(
    sub: &ArgMatches,
    entry_type: EntryType,
    schema_version: u32,
) -> Result<Payload, Failure> {
    match entry_type {
        EntryType::StageEvent => {
            // stage-event requires --stage and --summary, --payload optional.
            let stage = require_arg(sub, "stage")?;
            let summary = require_arg(sub, "summary")?;
            let raw_payload = sub.get_one::<String>("payload").map(String::as_str);
            Payload::stage_event_from_parts(schema_version, stage, summary, raw_payload)
                .map_err(Failure::Domain)
        }
        EntryType::Decision => {
            // decision requires --payload, rejects --stage and --summary.
            if sub.get_one::<String>("stage").is_some() {
                return Err(Failure::Invalid(
                    "entry type decision does not accept --stage".to_owned(),
                ));
            }
            if sub.get_one::<String>("summary").is_some() {
                return Err(Failure::Invalid(
                    "entry type decision does not accept --summary".to_owned(),
                ));
            }
            let raw = require_arg(sub, "payload")?;
            Payload::from_raw(entry_type, schema_version, &raw).map_err(Failure::Domain)
        }
        EntryType::CardCreated => Err(Failure::Domain(Error::NotAppendable { entry_type })),
    }
}

async fn append(sub: &ArgMatches, config: &Config) -> Result<Value, Failure> {
    let producer = require_producer(sub)?;
    let entry_type = EntryType::from_str(&require_arg(sub, "entry-type")?)?;
    // Rejected again in Store::append; refused here too so the CLI seam
    // reports it before any payload parsing (deep-review finding 1).
    if matches!(entry_type, EntryType::CardCreated) {
        return Err(Failure::Domain(Error::NotAppendable { entry_type }));
    }
    let schema_version = match sub.get_one::<String>("schema-version") {
        Some(text) => text.parse::<u32>().map_err(|_unparsed| {
            Failure::Invalid(format!(
                "schema-version must be an unsigned integer, got `{text}`"
            ))
        })?,
        None => CURRENT_SCHEMA_VERSION,
    };

    let payload = payload_from_args(sub, entry_type, schema_version)?;

    let card_id = CardId::from(require_arg(sub, "card-id")?);
    let store = open_store(config).await?;
    let accepted = store
        .append(AppendEntry {
            card_id: card_id.clone(),
            payload,
            producer,
            idempotency_key: sub.get_one::<String>("idempotency-key").cloned(),
        })
        .await?;
    Ok(json!({
        "card_id": card_id.to_string(),
        "entry_id": accepted.entry_id.to_string(),
        "sequence": accepted.sequence,
    }))
}

async fn history(sub: &ArgMatches, config: &Config) -> Result<Value, Failure> {
    let card_id = CardId::from(require_arg(sub, "card-id")?);
    let filter = match sub.get_one::<String>("entry-type") {
        Some(text) => Some(EntryType::from_str(text)?),
        None => None,
    };
    let store = open_store(config).await?;
    let entries = store.history(&card_id, filter).await?;
    let rendered = entries
        .iter()
        .map(|entry| {
            let mut entry_json = json!({
                "entry_id": entry.entry_id.to_string(),
                "card_id": entry.card_id.to_string(),
                "sequence": entry.sequence,
                "entry_type": entry.payload.entry_type().to_string(),
                "schema_version": entry.payload.schema_version(),
                "producer": {
                    "id": entry.producer.id,
                    "kind": entry.producer.kind.to_string(),
                },
                "recorded_at": render_timestamp(entry.recorded_at),
            });
            // Merge the payload fields from the library's history_fields().
            let fields = entry.payload.history_fields()?;
            if let serde_json::Value::Object(ref mut map) = entry_json {
                for (key, value) in fields {
                    map.insert(key, value);
                }
            }
            Ok(entry_json)
        })
        .collect::<Result<Vec<Value>, Error>>()?;
    Ok(json!({ "card_id": card_id.to_string(), "entries": rendered }))
}

async fn list(config: &Config) -> Result<Value, Failure> {
    let store = open_store(config).await?;
    let cards = store.list_cards().await?;
    let rendered: Vec<Value> = cards
        .iter()
        .map(|card| {
            json!({
                "card_id": card.card_id.to_string(),
                "title": card.title,
                "task_key": card.task_key,
                "workspace": card.workspace,
                "created_at": render_timestamp(card.created_at),
            })
        })
        .collect();
    Ok(json!({ "cards": rendered }))
}

/// RFC 3339 when it formats; the `Display` form as a readable fallback —
/// never empty, never fabricated.
fn render_timestamp(timestamp: time::OffsetDateTime) -> String {
    match timestamp.format(&Rfc3339) {
        Ok(rendered) => rendered,
        Err(_unformattable) => timestamp.to_string(),
    }
}
