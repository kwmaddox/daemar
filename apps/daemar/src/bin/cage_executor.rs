//! The cage's sole inhabitant: a static binary that executes exactly one
//! tool request per invocation and holds the container open between them.
//!
//! It must not load Config, call the provider, decrypt secrets, or inspect
//! the host environment — the cage has no key and no business having one.
//! The workspace is always /workspace: whatever the host mounted there is
//! the entire world, and the canonicalize+prefix confinement still applies
//! inside it — the cage is defense in depth, not a replacement.

use std::io::Read;
use std::path::Path;
use std::process::ExitCode;

use factory::executor::ToolRequest;
use factory::tools::{self, ToolContext, ToolOutcome};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        // The container's entrypoint: hold it open until torn down.
        Some("hold") => loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        },
        // One request: JSON in on stdin, JSON outcome out on stdout.
        Some("request") => {
            let mut buffer = String::new();
            if std::io::stdin().read_to_string(&mut buffer).is_err() {
                return ExitCode::from(2);
            }
            let outcome = serve(&buffer);
            println!(
                "{}",
                serde_json::to_string(&outcome).expect("an outcome serializes")
            );
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: cage-executor hold | cage-executor request");
            ExitCode::from(2)
        }
    }
}

/// Failures are outcomes, never crashes: the loop upstairs continues past
/// error content exactly as it does for in-process tools.
fn serve(request: &str) -> ToolOutcome {
    let refuse = |content: String| ToolOutcome {
        content,
        is_error: true,
        hash: String::new(),
        before_hash: None,
    };
    let request: ToolRequest = match serde_json::from_str(request) {
        Ok(request) => request,
        Err(error) => return refuse(format!("cage: unreadable tool request: {error}")),
    };
    let mut ctx = match ToolContext::new(Path::new("/workspace")) {
        Ok(ctx) => ctx,
        Err(error) => return refuse(format!("cage: no workspace: {error}")),
    };
    // The host's read record arrives as an expected hash: seed the fresh
    // context so the edit guard compares against what the model last saw.
    // The authoritative staleness check is edit's own re-hash of the file,
    // here, immediately before any bytes change.
    if let (Some(expected), Some(path)) = (
        request.expected_hash.as_ref(),
        request.args.get("path").and_then(|v| v.as_str()),
    ) {
        if let Ok(abs) = Path::new("/workspace").join(path).canonicalize() {
            ctx.read_hashes.insert(abs, expected.clone());
        }
    }
    tools::execute(&request.name, &request.args, &mut ctx, request.access)
}
