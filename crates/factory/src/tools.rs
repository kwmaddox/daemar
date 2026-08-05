//! The scout's tools: read-only reconnaissance over one territory.
//!
//! The sedai pattern, inherited: `ToolOutcome { content, is_error }` —
//! failures are content the loop continues past, never aborts. Plus two
//! daemar hardenings sedai (an interactive harness with a human watching)
//! never needed:
//!
//! 1. Confinement: every path canonicalizes and must live inside the
//!    territory root. Symlinks resolve before the check, so a link pointing
//!    out is refused. Outside-territory is an error outcome, and — per the
//!    audit rules — a logged one.
//! 2. Content hashes on every read: the epistemic pointer that goes on the
//!    ledger, so "what did the scout see" is answerable without copying
//!    bytes into the record. Doubles as the staleness guard when writes
//!    arrive (sedai's read_hashes, same idea).
//!
//! No shell. No writes. No network. Rung 1 by construction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

const READ_LINE_CAP: usize = 2000;
const READ_LINE_BYTES: usize = 2000;
const LIST_CAP: usize = 500;
const SEARCH_MATCH_CAP: usize = 50;
const SEARCH_FILE_BYTES: u64 = 1_000_000;

#[derive(Debug, Clone)]
pub struct ToolOutcome {
    pub content: String,
    pub is_error: bool,
    /// Content hash of what was read, when the tool read something whole.
    pub hash: String,
}

impl ToolOutcome {
    fn ok(content: impl Into<String>) -> Self {
        ToolOutcome {
            content: content.into(),
            is_error: false,
            hash: String::new(),
        }
    }
    fn error(content: impl Into<String>) -> Self {
        ToolOutcome {
            content: content.into(),
            is_error: true,
            hash: String::new(),
        }
    }
}

/// Per-flight tool state: the territory root and the read-hash record.
pub struct ToolContext {
    root: PathBuf,
    /// Path -> content hash at last successful read. The staleness guard
    /// writes will need; today, the audit's memory.
    pub read_hashes: HashMap<PathBuf, String>,
}

impl ToolContext {
    /// Root must exist; it is canonicalized once so every confinement check
    /// compares canonical-to-canonical.
    pub fn new(root: &Path) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|e| format!("territory {} cannot be resolved: {e}", root.display()))?;
        Ok(ToolContext {
            root,
            read_hashes: HashMap::new(),
        })
    }

    /// Resolve a path and confine it to the territory. Symlinks resolve
    /// first; anything landing outside is refused.
    fn confine(&self, path: &str) -> Result<PathBuf, String> {
        let joined = {
            let p = Path::new(path);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                self.root.join(p)
            }
        };
        let canonical = joined
            .canonicalize()
            .map_err(|e| format!("cannot resolve '{path}': {e}"))?;
        if canonical.starts_with(&self.root) {
            Ok(canonical)
        } else {
            Err(format!("'{path}' is outside the territory"))
        }
    }

    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .display()
            .to_string()
    }
}

/// The specs advertised to the model, in the provider's tools shape.
pub fn specs() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "read",
                "description": "Read a file's contents with line numbers. Paths are relative to the territory root.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "File path, relative to the territory root."},
                        "offset": {"type": "integer", "description": "1-based start line (optional)."},
                        "limit": {"type": "integer", "description": "Max lines to return (optional, capped)."}
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_files",
                "description": "List files and directories (gitignore-aware, hidden files skipped). Directories end with '/'.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Directory to list (default: territory root)."},
                        "recursive": {"type": "boolean", "description": "Recurse into subdirectories (default false)."}
                    }
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "search",
                "description": "Search file contents for a substring (case-sensitive, gitignore-aware). Returns path:line: text matches.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": {"type": "string", "description": "Substring to find."},
                        "path": {"type": "string", "description": "Directory to search under (default: territory root)."}
                    },
                    "required": ["pattern"]
                }
            }
        }
    ])
}

/// Dispatch. Unknown tools and malformed args are error outcomes; the loop
/// continues either way, and every outcome lands on the ledger.
pub fn execute(name: &str, args: &Value, ctx: &mut ToolContext) -> ToolOutcome {
    match name {
        "read" => read(args, ctx),
        "list_files" => list_files(args, ctx),
        "search" => search(args, ctx),
        other => ToolOutcome::error(format!("unknown tool '{other}'")),
    }
}

fn arg_str<'v>(args: &'v Value, key: &str) -> Option<&'v str> {
    args.get(key).and_then(Value::as_str)
}

// ── read ─────────────────────────────────────────────────────────────────────

fn read(args: &Value, ctx: &mut ToolContext) -> ToolOutcome {
    let Some(path) = arg_str(args, "path") else {
        return ToolOutcome::error("read: missing required 'path'");
    };
    let offset = args
        .get("offset")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .max(1) as usize;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(READ_LINE_CAP)
        .clamp(1, READ_LINE_CAP);

    let abs = match ctx.confine(path) {
        Ok(abs) => abs,
        Err(e) => return ToolOutcome::error(format!("read: {e}")),
    };
    if abs.is_dir() {
        return ToolOutcome::error(format!("read: '{path}' is a directory"));
    }
    let bytes = match std::fs::read(&abs) {
        Ok(b) => b,
        Err(e) => return ToolOutcome::error(format!("read: cannot read '{path}': {e}")),
    };
    if bytes.contains(&0) {
        return ToolOutcome::error(format!("read: '{path}' appears to be binary"));
    }
    let hash = content_hash(&bytes);
    ctx.read_hashes.insert(abs, hash.clone());

    let text = String::from_utf8_lossy(&bytes);
    let mut out = String::new();
    let mut shown = 0usize;
    for (idx, line) in text.lines().enumerate() {
        let lineno = idx + 1;
        if lineno < offset {
            continue;
        }
        if shown >= limit {
            out.push_str(&format!("... (truncated at {limit} lines)\n"));
            break;
        }
        let mut line = line.to_string();
        if line.len() > READ_LINE_BYTES {
            truncate_at_boundary(&mut line, READ_LINE_BYTES);
            line.push_str(" …(line truncated)");
        }
        out.push_str(&format!("{lineno:>6}\t{line}\n"));
        shown += 1;
    }
    if shown == 0 {
        out.push_str("(no lines in range; file may be empty)\n");
    }
    ToolOutcome {
        content: out,
        is_error: false,
        hash,
    }
}

// ── list_files ───────────────────────────────────────────────────────────────

fn list_files(args: &Value, ctx: &mut ToolContext) -> ToolOutcome {
    let path = arg_str(args, "path").unwrap_or(".");
    let recursive = args
        .get("recursive")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let abs = match ctx.confine(path) {
        Ok(abs) => abs,
        Err(e) => return ToolOutcome::error(format!("list_files: {e}")),
    };
    if !abs.is_dir() {
        return ToolOutcome::error(format!("list_files: '{path}' is not a directory"));
    }
    let mut walker = ignore::WalkBuilder::new(&abs);
    walker.require_git(false);
    if !recursive {
        walker.max_depth(Some(1));
    }
    let mut entries: Vec<String> = Vec::new();
    let mut unreadable = 0usize;
    for entry in walker.build() {
        // Traversal errors are counted, never silently flattened away —
        // partial results must say they are partial.
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                unreadable += 1;
                continue;
            }
        };
        let p = entry.path();
        if p == abs {
            continue;
        }
        let mut rel = ctx.relative(p);
        if p.is_dir() {
            rel.push('/');
        }
        entries.push(rel);
        if entries.len() >= LIST_CAP {
            break;
        }
    }
    entries.sort();
    let mut out = entries.join("\n");
    if entries.len() >= LIST_CAP {
        out.push_str(&format!("\n... (truncated at {LIST_CAP} entries)"));
    }
    if out.is_empty() {
        out.push_str("(empty)");
    }
    if unreadable > 0 {
        out.push_str(&format!("\n({unreadable} entries unreadable)"));
    }
    ToolOutcome::ok(out)
}

// ── search ───────────────────────────────────────────────────────────────────

fn search(args: &Value, ctx: &mut ToolContext) -> ToolOutcome {
    let Some(pattern) = arg_str(args, "pattern").filter(|p| !p.is_empty()) else {
        return ToolOutcome::error("search: missing required 'pattern'");
    };
    let path = arg_str(args, "path").unwrap_or(".");
    let abs = match ctx.confine(path) {
        Ok(abs) => abs,
        Err(e) => return ToolOutcome::error(format!("search: {e}")),
    };
    let mut matches: Vec<String> = Vec::new();
    let mut truncated = false;
    let mut unreadable = 0usize;
    'files: for entry in ignore::WalkBuilder::new(&abs).require_git(false).build() {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                unreadable += 1;
                continue;
            }
        };
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        if entry
            .metadata()
            .map(|m| m.len() > SEARCH_FILE_BYTES)
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(p) else {
            unreadable += 1;
            continue;
        };
        if bytes.contains(&0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (idx, line) in text.lines().enumerate() {
            if line.contains(pattern) {
                let mut shown = line.trim().to_string();
                if shown.len() > 200 {
                    truncate_at_boundary(&mut shown, 200);
                    shown.push('…');
                }
                matches.push(format!("{}:{}: {shown}", ctx.relative(p), idx + 1));
                if matches.len() >= SEARCH_MATCH_CAP {
                    truncated = true;
                    break 'files;
                }
            }
        }
    }
    let note = if unreadable > 0 {
        format!("\n({unreadable} entries unreadable)")
    } else {
        String::new()
    };
    if matches.is_empty() {
        return ToolOutcome::ok(format!("no matches for '{pattern}'{note}"));
    }
    let mut out = matches.join("\n");
    if truncated {
        out.push_str(&format!("\n... (truncated at {SEARCH_MATCH_CAP} matches)"));
    }
    out.push_str(&note);
    ToolOutcome::ok(out)
}

/// Byte-bounded truncation that never splits a UTF-8 sequence.
/// `String::truncate` panics off a char boundary — a scout reading a source
/// file with a long non-ASCII line would crash on valid input. (Caught by
/// review; inherited from sedai's read tool, which has the same latent bug.)
fn truncate_at_boundary(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    let mut cut = max_bytes;
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    text.truncate(cut);
}

/// Content hash: sha256, truncated to 16 hex chars — enough to pin what was
/// seen without bloating events.
pub fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())[..16].to_string()
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn territory(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("daemar-tools-{}-{name}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "pub fn answer() {}\n").unwrap();
        std::fs::write(dir.join(".gitignore"), "target/\n").unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("target/junk.txt"), "ignored\n").unwrap();
        dir
    }

    #[test]
    fn paths_are_confined_to_the_territory() {
        let dir = territory("confine");
        let mut ctx = ToolContext::new(&dir).unwrap();
        let out = execute("read", &json!({"path": "../../etc/hosts"}), &mut ctx);
        assert!(out.is_error);
        assert!(
            out.content.contains("outside the territory") || out.content.contains("cannot resolve")
        );
        // Absolute paths outside are refused too.
        let out = execute("read", &json!({"path": "/etc/hosts"}), &mut ctx);
        assert!(
            out.is_error,
            "absolute escape must be refused: {}",
            out.content
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn symlinks_pointing_out_are_refused() {
        let dir = territory("symlink");
        let outside = std::env::temp_dir().join(format!("daemar-outside-{}", std::process::id()));
        std::fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("sneaky")).unwrap();
        let mut ctx = ToolContext::new(&dir).unwrap();
        let out = execute("read", &json!({"path": "sneaky"}), &mut ctx);
        assert!(
            out.is_error,
            "symlink escape must be refused: {}",
            out.content
        );
        std::fs::remove_dir_all(&dir).ok();
        std::fs::remove_file(&outside).ok();
    }

    #[test]
    fn read_returns_numbered_lines_and_records_a_hash() {
        let dir = territory("read");
        let mut ctx = ToolContext::new(&dir).unwrap();
        let out = execute("read", &json!({"path": "src/lib.rs"}), &mut ctx);
        assert!(!out.is_error);
        assert!(out.content.contains("1\tpub fn answer()"));
        assert_eq!(out.hash.len(), 16);
        assert_eq!(ctx.read_hashes.len(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn listing_honors_gitignore_and_search_finds_the_needle() {
        let dir = territory("walk");
        let mut ctx = ToolContext::new(&dir).unwrap();
        let out = execute("list_files", &json!({"recursive": true}), &mut ctx);
        assert!(!out.is_error);
        assert!(out.content.contains("src/lib.rs"));
        assert!(
            !out.content.contains("junk.txt"),
            "gitignored files must not list"
        );

        let out = execute("search", &json!({"pattern": "answer"}), &mut ctx);
        assert!(!out.is_error);
        assert!(out.content.contains("src/lib.rs:1:"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn long_multibyte_lines_truncate_without_panicking() {
        let dir = territory("multibyte");
        // '€' is 3 bytes; 700 of them = 2100 bytes, and byte 2000 falls
        // mid-sequence — the exact input the old truncate panicked on.
        std::fs::write(dir.join("src/unicode.rs"), "€".repeat(700)).unwrap();
        let mut ctx = ToolContext::new(&dir).unwrap();
        let out = execute("read", &json!({"path": "src/unicode.rs"}), &mut ctx);
        assert!(!out.is_error, "{}", out.content);
        assert!(out.content.contains("(line truncated)"));

        let out = execute("search", &json!({"pattern": "€"}), &mut ctx);
        assert!(!out.is_error, "{}", out.content);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_tools_are_error_outcomes_not_aborts() {
        let dir = territory("unknown");
        let mut ctx = ToolContext::new(&dir).unwrap();
        let out = execute("teleport", &json!({}), &mut ctx);
        assert!(out.is_error);
        std::fs::remove_dir_all(&dir).ok();
    }
}
