//! The factory's runner.
//!
//!     daemar "<request>"                       one-phase prompt workflow
//!     ... | daemar -                           request from stdin
//!     daemar plan [--repo <path>] "<request>"  grounded plan, then cock at plan->respond
//!     daemar scout [--repo <path>] "<question>" read-only recon over the territory
//!     daemar grant <slip-id>                   controller clears the boundary
//!     daemar refuse <slip-id> [why]            controller refuses; slip closes rejected
//!     daemar continue <slip-id>                fly the next phase from the printout
//!     daemar dispose <slip-id> [why]           close a flight that could not close itself
//!
//! Exit-and-resume at boundaries, by ruling: a flight that hits a clearance
//! EXITS. The strip cocks on the board; `continue` later rebuilds context
//! purely from the ledger — the printout — in a fresh process, possibly on a
//! different airframe. The ledger is the memory; sessions are caches.
//!
//! Failure must be witnessed: on error the runner reports, ends the phase in
//! error, and leaves the slip OPEN for the controller's disposition.

use std::process::ExitCode;

mod config;
mod engine;
mod pens;
mod provider;
mod registry;
mod roster;
mod tools;
mod workflows;

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("dispose") => pens::dispose(&args[1..]),
        Some("grant") => pens::grant(&args[1..]),
        Some("refuse") => pens::refuse(&args[1..]),
        Some("continue") => workflows::continue_flight(&args[1..]),
        Some("plan") => territory_dispatch(&args[1..], workflows::plan_flight),
        Some("scout") => territory_dispatch(&args[1..], workflows::scout_flight),
        _ => match read_request(&args) {
            Some(request) => workflows::prompt_flight(&request),
            None => usage(),
        },
    }
}

pub fn usage() -> ExitCode {
    eprintln!(
        "usage: daemar \"<request>\"            (or ... | daemar -)\n       \
         daemar plan [--repo <path>] \"<request>\"\n       \
         daemar scout [--repo <path>] \"<question>\"\n       \
         daemar grant|refuse|continue|dispose <slip-id> [\"<reason>\"]"
    );
    ExitCode::from(2)
}

/// The request from args, or stdin when the sole arg is '-'. None = usage.
fn read_request(args: &[String]) -> Option<String> {
    let request = if args.len() == 1 && args[0] == "-" {
        let mut buffer = String::new();
        use std::io::Read;
        std::io::stdin().read_to_string(&mut buffer).ok()?;
        buffer
    } else {
        args.join(" ")
    };
    if request.trim().is_empty() {
        None
    } else {
        Some(request)
    }
}

/// `[--repo <path>] "<request>"` — the shared front door for every workflow
/// that flies over a territory. Defaults to where the engineer is standing.
fn territory_dispatch(args: &[String], fly: fn(&str, &str) -> ExitCode) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("--repo") => {
            // A --repo with no path must not fall through and fly the flag
            // as the request — that would mint a real, paid, nonsense slip.
            if args.len() < 2 {
                return usage();
            }
            match read_request(&args[2..]) {
                Some(request) => fly(&request, &args[1]),
                None => usage(),
            }
        }
        Some(flag) if flag.starts_with("--") => usage(),
        _ => match read_request(args) {
            Some(request) => fly(&request, "."),
            None => usage(),
        },
    }
}
