//! The factory's CLI: a thin skin over the `factory` crate. Arg parsing and
//! presentation live here and nowhere else; the MCP tower (`daemar mcp`) is
//! the second skin over the same library seams.
//!
//!     daemar "<request>"                       one-phase prompt workflow
//!     ... | daemar -                           request from stdin
//!     daemar plan [--repo <path>] "<request>"  grounded plan, then cock at plan->respond
//!     daemar scout [--repo <path>] "<question>" read-only recon over the territory
//!     daemar grant <slip-id>                   controller clears the boundary
//!     daemar refuse <slip-id> [why]            controller refuses; slip closes rejected
//!     daemar continue <slip-id>                fly the next phase from the printout
//!     daemar dispose <slip-id> [why]           close a flight that could not close itself
//!     daemar mcp                               serve the tower over stdio (MCP)
//!
//! Exit-and-resume at boundaries, by ruling: a flight that hits a clearance
//! EXITS. The strip cocks on the board; `continue` later rebuilds context
//! purely from the ledger — the printout — in a fresh process, possibly on a
//! different airframe. The ledger is the memory; sessions are caches.
//!
//! Failure must be witnessed: on error the runner reports, ends the phase in
//! error, and leaves the slip OPEN for the controller's disposition.

use std::process::ExitCode;

use factory::config::Config;
use factory::pens;
use factory::workflows::{self, FlightError, FlightReport};

mod mcp;

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("mcp") => mcp::serve(),
        Some("dispose") => pen_dispatch(&args[1..], pens::dispose),
        Some("grant") => pen_dispatch(&args[1..], pens::grant),
        Some("refuse") => pen_dispatch(&args[1..], pens::refuse),
        Some("continue") => pen_dispatch(&args[1..], |slip_id, _| {
            fly(|config| workflows::continue_flight(config, slip_id))
        }),
        Some("plan") => territory_dispatch(&args[1..], |request, repo| {
            fly(|config| workflows::plan_flight(config, request, repo))
        }),
        Some("scout") => territory_dispatch(&args[1..], |request, repo| {
            fly(|config| workflows::scout_flight(config, request, repo))
        }),
        _ => match read_request(&args) {
            Some(request) => fly(|config| workflows::prompt_flight(config, &request)),
            None => usage(),
        },
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: daemar \"<request>\"            (or ... | daemar -)\n       \
         daemar plan [--repo <path>] \"<request>\"\n       \
         daemar scout [--repo <path>] \"<question>\"\n       \
         daemar grant|refuse|continue|dispose <slip-id> [\"<reason>\"]\n       \
         daemar mcp"
    );
    ExitCode::from(2)
}

/// Run one flight and present its report: response text on stdout, the
/// slip's footer on stderr, failures named with their remedy.
fn fly(flight: impl FnOnce(&Config) -> Result<FlightReport, FlightError>) -> ExitCode {
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("daemar: {error}");
            return ExitCode::from(2);
        }
    };
    match flight(&config) {
        Ok(report) => {
            println!("{}", report.text.trim_end());
            let id = &report.slip_id;
            match report.cocked_at {
                Some(boundary) => eprintln!(
                    "\nslip {id} · COCKED at {boundary} · {} tokens · ${:.4} · {} turn(s) · \
                     daemar grant {id}  |  daemar refuse {id} \"<reason>\" · board: /slip/{id}",
                    report.tokens, report.cost, report.turns
                ),
                None => eprintln!(
                    "\nslip {id} · accepted · {} tokens · ${:.4} · {} turn(s) · board: /slip/{id}",
                    report.tokens, report.cost, report.turns
                ),
            }
            ExitCode::SUCCESS
        }
        Err(FlightError::Refused(message)) => {
            eprintln!("daemar: {message}");
            ExitCode::from(2)
        }
        Err(FlightError::Failed { slip_id }) => {
            eprintln!(
                "slip {slip_id} · FAILED, left open for disposition · \
                 daemar dispose {slip_id} \"<reason>\" · board: /slip/{slip_id}"
            );
            ExitCode::FAILURE
        }
        Err(FlightError::Ledger(error)) => {
            eprintln!("daemar: ledger failure: {error}");
            ExitCode::FAILURE
        }
    }
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

/// `<slip-id> ["<reason>"]` — the shared front door for every command that
/// writes on an existing slip.
fn pen_dispatch(args: &[String], pen: impl FnOnce(&str, &str) -> ExitCode) -> ExitCode {
    let Some(slip_id) = args.first().filter(|a| !a.trim().is_empty()) else {
        return usage();
    };
    pen(slip_id, &args[1..].join(" "))
}

/// `[--repo <path>] "<request>"` — the shared front door for every workflow
/// that flies over a territory. Defaults to where the engineer is standing.
fn territory_dispatch(args: &[String], fly: impl FnOnce(&str, &str) -> ExitCode) -> ExitCode {
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
