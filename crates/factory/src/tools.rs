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

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::roster::ToolAccess;

const READ_LINE_CAP: usize = 2000;
const READ_LINE_BYTES: usize = 2000;
const LIST_CAP: usize = 500;
const SEARCH_MATCH_CAP: usize = 50;
const SEARCH_FILE_BYTES: u64 = 1_000_000;

/// Serde derives: the outcome crosses the cage boundary as JSON, losslessly
/// — the executor seam must be invisible to the ledger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutcome {
    pub content: String,
    pub is_error: bool,
    /// Content hash of what was read — or, for a mutation, the post-image.
    pub hash: String,
    /// The pre-image hash of a mutation, when there was one: edit records
    /// what it replaced; a new-file write has no pre-image. Defaulted so
    /// the outcome crosses old cage boundaries unchanged.
    #[serde(default)]
    pub before_hash: Option<String>,
}

impl ToolOutcome {
    fn ok(content: impl Into<String>) -> Self {
        ToolOutcome {
            content: content.into(),
            is_error: false,
            hash: String::new(),
            before_hash: None,
        }
    }
    fn error(content: impl Into<String>) -> Self {
        ToolOutcome {
            content: content.into(),
            is_error: true,
            hash: String::new(),
            before_hash: None,
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

    /// Confine a path that need not exist yet: the PARENT must exist and
    /// canonicalize inside the territory; the final component must be a
    /// plain name. Symlinked parents resolve before the check, same as
    /// confine — a link pointing out is refused.
    fn confine_new(&self, path: &str) -> Result<PathBuf, String> {
        let p = Path::new(path);
        if p.is_absolute() {
            return Err(format!("'{path}' must be relative"));
        }
        let joined = self.root.join(p);
        let Some(name) = joined.file_name().map(std::ffi::OsStr::to_os_string) else {
            return Err(format!("'{path}' has no file name"));
        };
        let parent = joined.parent().unwrap_or(&self.root);
        let canonical_parent = parent
            .canonicalize()
            .map_err(|e| format!("cannot resolve parent of '{path}': {e}"))?;
        if !canonical_parent.starts_with(&self.root) {
            return Err(format!("'{path}' is outside the territory"));
        }
        Ok(canonical_parent.join(name))
    }

    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .display()
            .to_string()
    }
}

/// The specs advertised to the model, in the Responses API tools shape:
/// function fields at TOP level, not nested under a "function" object as
/// chat-completions had them. The surface follows the seat's capability:
/// advertisement AND dispatch are both gated, so an unadvertised call from
/// a lesser seat is refused, not executed.
pub fn specs(access: ToolAccess) -> Value {
    let mut tools = read_specs();
    if access == ToolAccess::ReadWrite {
        if let (Value::Array(all), Value::Array(write)) = (&mut tools, write_specs()) {
            all.extend(write);
        }
    }
    tools
}

fn read_specs() -> Value {
    json!([
        {
            "type": "function",
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
            },
            "strict": false
        },
        {
            "type": "function",
            "name": "list_files",
            "description": "List files and directories (gitignore-aware, hidden files skipped). Directories end with '/'.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory to list (default: territory root)."},
                    "recursive": {"type": "boolean", "description": "Recurse into subdirectories (default false)."}
                }
            },
            "strict": false
        },
        {
            "type": "function",
            "name": "search",
            "description": "Search file contents for a substring (case-sensitive, gitignore-aware). Returns path:line: text matches.",
            "parameters": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Substring to find."},
                    "path": {"type": "string", "description": "Directory to search under (default: territory root)."}
                },
                "required": ["pattern"]
            },
            "strict": false
        }
    ])
}

fn write_specs() -> Value {
    json!([
        {
            "type": "function",
            "name": "edit",
            "description": "Replace ONE exact occurrence of `old` with `new` in a file you have already read. Refused if the file was not read, changed since your last read, or `old` matches zero or multiple times.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path, relative to the territory root."},
                    "old": {"type": "string", "description": "Exact text to replace; must occur exactly once."},
                    "new": {"type": "string", "description": "Replacement text."}
                },
                "required": ["path", "old", "new"]
            },
            "strict": false
        },
        {
            "type": "function",
            "name": "write",
            "description": "Create a NEW file with the given content. Refused if the path already exists — mutation belongs to edit.",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "New file path, relative to the territory root."},
                    "content": {"type": "string", "description": "Full file content."}
                },
                "required": ["path", "content"]
            },
            "strict": false
        }
    ])
}

/// Dispatch, capability-gated. Unknown tools, forbidden tools, and
/// malformed args are error outcomes; the loop continues either way, and
/// every outcome lands on the ledger. There is NO delete: the capability
/// does not exist, structurally.
pub fn execute(name: &str, args: &Value, ctx: &mut ToolContext, access: ToolAccess) -> ToolOutcome {
    let readable = matches!(access, ToolAccess::ReadOnly | ToolAccess::ReadWrite);
    let writable = access == ToolAccess::ReadWrite;
    match name {
        "read" if readable => read(args, ctx),
        "list_files" if readable => list_files(args, ctx),
        "search" if readable => search(args, ctx),
        "edit" if writable => edit(args, ctx),
        "write" if writable => write_new(args, ctx),
        "read" | "list_files" | "search" | "edit" | "write" => {
            ToolOutcome::error(format!("forbidden: this seat may not call '{name}'"))
        }
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
        before_hash: None,
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

// ── edit / write: the hands, hash-guarded ────────────────────────────────────

/// Replace exactly one occurrence of `old` with `new`, guarded by the
/// read-hash record: a file not read, or changed since its last read, is a
/// refusal telling the model to read again. The guard entry is NOT advanced
/// by the edit — a second edit demands a fresh read of the mutated file.
fn edit(args: &Value, ctx: &mut ToolContext) -> ToolOutcome {
    let (Some(path), Some(old), Some(new)) = (
        arg_str(args, "path"),
        arg_str(args, "old"),
        arg_str(args, "new"),
    ) else {
        return ToolOutcome::error("edit: 'path', 'old', and 'new' are all required");
    };
    if old.is_empty() {
        return ToolOutcome::error("edit: 'old' must not be empty");
    }
    let abs = match ctx.confine(path) {
        Ok(abs) => abs,
        Err(e) => return ToolOutcome::error(format!("edit: {e}")),
    };
    if abs.is_dir() {
        return ToolOutcome::error(format!("edit: '{path}' is a directory"));
    }
    let bytes = match std::fs::read(&abs) {
        Ok(b) => b,
        Err(e) => return ToolOutcome::error(format!("edit: cannot read '{path}': {e}")),
    };
    if bytes.contains(&0) {
        return ToolOutcome::error(format!("edit: '{path}' appears to be binary"));
    }
    let current = content_hash(&bytes);
    match ctx.read_hashes.get(&abs) {
        None => {
            return ToolOutcome::error(format!("edit: '{path}' has not been read — read it first"))
        }
        Some(seen) if *seen != current => {
            return ToolOutcome::error(format!(
                "edit: '{path}' changed since it was read — read it again"
            ))
        }
        Some(_) => {}
    }
    let text = String::from_utf8_lossy(&bytes).to_string();
    let occurrences = text.matches(old).count();
    if occurrences == 0 {
        return ToolOutcome::error(format!("edit: 'old' does not occur in '{path}'"));
    }
    if occurrences > 1 {
        return ToolOutcome::error(format!(
            "edit: 'old' occurs {occurrences} times in '{path}' — make it unique"
        ));
    }
    let updated = text.replacen(old, new, 1);
    if let Err(e) = std::fs::write(&abs, &updated) {
        return ToolOutcome::error(format!("edit: cannot write '{path}': {e}"));
    }
    let post = content_hash(updated.as_bytes());
    ToolOutcome {
        content: format!("edited '{path}': replaced 1 occurrence"),
        is_error: false,
        hash: post,
        before_hash: Some(current),
    }
}

/// Create a NEW file. An existing path — including one racing into
/// existence — is refused with create-new semantics: mutation belongs to
/// edit, and nothing is ever silently overwritten.
fn write_new(args: &Value, ctx: &mut ToolContext) -> ToolOutcome {
    let (Some(path), Some(content)) = (arg_str(args, "path"), arg_str(args, "content")) else {
        return ToolOutcome::error("write: 'path' and 'content' are required");
    };
    let abs = match ctx.confine_new(path) {
        Ok(abs) => abs,
        Err(e) => return ToolOutcome::error(format!("write: {e}")),
    };
    let mut file = match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&abs)
    {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return ToolOutcome::error(format!(
                "write: '{path}' already exists — use edit to change it"
            ))
        }
        Err(e) => return ToolOutcome::error(format!("write: cannot create '{path}': {e}")),
    };
    use std::io::Write as _;
    if let Err(e) = file.write_all(content.as_bytes()) {
        return ToolOutcome::error(format!("write: cannot write '{path}': {e}"));
    }
    ToolOutcome {
        content: format!("wrote '{path}' ({} bytes)", content.len()),
        is_error: false,
        hash: content_hash(content.as_bytes()),
        before_hash: None,
    }
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
        let out = execute(
            "read",
            &json!({"path": "../../etc/hosts"}),
            &mut ctx,
            ToolAccess::ReadOnly,
        );
        assert!(out.is_error);
        assert!(
            out.content.contains("outside the territory") || out.content.contains("cannot resolve")
        );
        // Absolute paths outside are refused too.
        let out = execute(
            "read",
            &json!({"path": "/etc/hosts"}),
            &mut ctx,
            ToolAccess::ReadOnly,
        );
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
        let out = execute(
            "read",
            &json!({"path": "sneaky"}),
            &mut ctx,
            ToolAccess::ReadOnly,
        );
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
        let out = execute(
            "read",
            &json!({"path": "src/lib.rs"}),
            &mut ctx,
            ToolAccess::ReadOnly,
        );
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
        let out = execute(
            "list_files",
            &json!({"recursive": true}),
            &mut ctx,
            ToolAccess::ReadOnly,
        );
        assert!(!out.is_error);
        assert!(out.content.contains("src/lib.rs"));
        assert!(
            !out.content.contains("junk.txt"),
            "gitignored files must not list"
        );

        let out = execute(
            "search",
            &json!({"pattern": "answer"}),
            &mut ctx,
            ToolAccess::ReadOnly,
        );
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
        let out = execute(
            "read",
            &json!({"path": "src/unicode.rs"}),
            &mut ctx,
            ToolAccess::ReadOnly,
        );
        assert!(!out.is_error, "{}", out.content);
        assert!(out.content.contains("(line truncated)"));

        let out = execute(
            "search",
            &json!({"pattern": "€"}),
            &mut ctx,
            ToolAccess::ReadOnly,
        );
        assert!(!out.is_error, "{}", out.content);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_tools_are_error_outcomes_not_aborts() {
        let dir = territory("unknown");
        let mut ctx = ToolContext::new(&dir).unwrap();
        let out = execute("teleport", &json!({}), &mut ctx, ToolAccess::ReadOnly);
        assert!(out.is_error);
        std::fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn the_write_surface_is_capability_gated_and_has_no_delete() {
        let dir = territory("caps");
        let mut ctx = ToolContext::new(&dir).unwrap();
        // A read-only seat may not mutate, even by naming the tool.
        let out = execute(
            "edit",
            &json!({"path": "src/lib.rs", "old": "answer", "new": "reply"}),
            &mut ctx,
            ToolAccess::ReadOnly,
        );
        assert!(
            out.is_error && out.content.contains("forbidden"),
            "{}",
            out.content
        );
        // The advertised ReadWrite surface is exactly the allowed set —
        // delete does not exist, structurally (the moghedien pattern).
        let names: Vec<String> = specs(ToolAccess::ReadWrite)
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(names, ["read", "list_files", "search", "edit", "write"]);
        assert!(!names
            .iter()
            .any(|n| n.contains("delete") || n.contains("remove")));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn edit_refuses_unread_and_stale_files_then_replaces_exactly_once() {
        let dir = territory("edit");
        let mut ctx = ToolContext::new(&dir).unwrap();
        let args = json!({"path": "src/lib.rs", "old": "answer", "new": "reply"});

        // Unread: refused with instructions.
        let out = execute("edit", &args, &mut ctx, ToolAccess::ReadWrite);
        assert!(
            out.is_error && out.content.contains("read it first"),
            "{}",
            out.content
        );

        // Read, then externally changed: stale, refused.
        execute(
            "read",
            &json!({"path": "src/lib.rs"}),
            &mut ctx,
            ToolAccess::ReadWrite,
        );
        std::fs::write(dir.join("src/lib.rs"), "pub fn answer() -> u8 { 43 }\n").unwrap();
        let out = execute("edit", &args, &mut ctx, ToolAccess::ReadWrite);
        assert!(
            out.is_error && out.content.contains("read it again"),
            "{}",
            out.content
        );

        // Fresh read, then a clean single replacement with both hashes.
        execute(
            "read",
            &json!({"path": "src/lib.rs"}),
            &mut ctx,
            ToolAccess::ReadWrite,
        );
        let out = execute("edit", &args, &mut ctx, ToolAccess::ReadWrite);
        assert!(!out.is_error, "{}", out.content);
        assert_eq!(out.hash.len(), 16, "post-image hash");
        assert_eq!(
            out.before_hash.as_ref().map(String::len),
            Some(16),
            "pre-image hash"
        );
        let now = std::fs::read_to_string(dir.join("src/lib.rs")).unwrap();
        assert!(now.contains("reply") && !now.contains("answer"));

        // The guard did NOT advance: editing again without re-reading refuses.
        let out = execute(
            "edit",
            &json!({"path": "src/lib.rs", "old": "reply", "new": "answer"}),
            &mut ctx,
            ToolAccess::ReadWrite,
        );
        assert!(
            out.is_error && out.content.contains("read it again"),
            "{}",
            out.content
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn edit_refuses_ambiguity_and_absence() {
        let dir = territory("edit-ambig");
        std::fs::write(dir.join("src/lib.rs"), "aa aa\n").unwrap();
        let mut ctx = ToolContext::new(&dir).unwrap();
        execute(
            "read",
            &json!({"path": "src/lib.rs"}),
            &mut ctx,
            ToolAccess::ReadWrite,
        );
        let out = execute(
            "edit",
            &json!({"path": "src/lib.rs", "old": "aa", "new": "b"}),
            &mut ctx,
            ToolAccess::ReadWrite,
        );
        assert!(
            out.is_error && out.content.contains("2 times"),
            "{}",
            out.content
        );
        let out = execute(
            "edit",
            &json!({"path": "src/lib.rs", "old": "zz", "new": "b"}),
            &mut ctx,
            ToolAccess::ReadWrite,
        );
        assert!(
            out.is_error && out.content.contains("does not occur"),
            "{}",
            out.content
        );
        let out = execute(
            "edit",
            &json!({"path": "src/lib.rs", "old": "", "new": "b"}),
            &mut ctx,
            ToolAccess::ReadWrite,
        );
        assert!(
            out.is_error && out.content.contains("must not be empty"),
            "{}",
            out.content
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_creates_new_files_only_and_stays_confined() {
        let dir = territory("write");
        let mut ctx = ToolContext::new(&dir).unwrap();
        let out = execute(
            "write",
            &json!({"path": "src/new_module.rs", "content": "pub fn fresh() {}\n"}),
            &mut ctx,
            ToolAccess::ReadWrite,
        );
        assert!(!out.is_error, "{}", out.content);
        assert_eq!(out.hash.len(), 16);
        assert_eq!(out.before_hash, None, "a new file has no pre-image");
        assert!(dir.join("src/new_module.rs").exists());

        // Existing file: refused — mutation belongs to edit.
        let out = execute(
            "write",
            &json!({"path": "src/lib.rs", "content": "clobber"}),
            &mut ctx,
            ToolAccess::ReadWrite,
        );
        assert!(
            out.is_error && out.content.contains("already exists"),
            "{}",
            out.content
        );

        // Escapes: refused.
        let out = execute(
            "write",
            &json!({"path": "../outside.rs", "content": "x"}),
            &mut ctx,
            ToolAccess::ReadWrite,
        );
        assert!(out.is_error, "{}", out.content);
        std::fs::remove_dir_all(&dir).ok();
    }
}
