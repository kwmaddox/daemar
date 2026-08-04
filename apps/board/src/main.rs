//! The board: the controller's instrument.
//!
//! Renders slip faces in bays (cocked first — they are literally tilted),
//! with progressive disclosure: face → sections/phases/clearances → raw
//! ledger. Reads `*.jsonl` ledgers from a directory on every request; the
//! files are the truth and the board holds no state of its own.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use ledger::{Slip, Status};

struct App {
    ledgers: PathBuf,
}

#[tokio::main]
async fn main() {
    let ledgers = std::env::var("DAEMAR_LEDGERS").unwrap_or_else(|_| "fixtures".to_string());
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(4700);
    let app = Arc::new(App { ledgers: PathBuf::from(ledgers) });

    let router = Router::new()
        .route("/", get(index))
        .route("/board", get(board_fragment))
        .route("/slip/{id}", get(slip_detail))
        .with_state(app.clone());

    let addr = format!("127.0.0.1:{port}");
    println!("[board] serving http://{addr}  ledgers: {}", app.ledgers.display());
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    axum::serve(listener, router).await.expect("serve");
}

fn load(app: &App) -> Vec<(Slip, Vec<ledger::Event>)> {
    ledger::load_dir(&app.ledgers).unwrap_or_default()
}

// ── Pages ────────────────────────────────────────────────────────────────────

async fn index(State(app): State<Arc<App>>) -> Html<String> {
    let board = render_board(&load(&app));
    Html(format!(
        "{STYLE}<title>daemar — the board</title>\
         <header><h1>daemar</h1><span class=\"sub\">the board · ledgers are truth · slips are folds</span></header>\
         <main id=\"board\">{board}</main>\
         <script>setInterval(async()=>{{const r=await fetch('/board');\
         document.getElementById('board').innerHTML=await r.text();}},2000);</script>"
    ))
}

async fn board_fragment(State(app): State<Arc<App>>) -> Html<String> {
    Html(render_board(&load(&app)))
}

async fn slip_detail(State(app): State<Arc<App>>, Path(id): Path<String>) -> Response {
    let all = load(&app);
    let Some((slip, events)) = all.iter().find(|(s, _)| s.id == id) else {
        return (
            StatusCode::NOT_FOUND,
            Html(format!("{STYLE}<main><p>no slip {}</p><p><a href=\"/\">← board</a></p></main>", esc(&id))),
        )
            .into_response();
    };
    Html(format!(
        "{STYLE}<title>slip {}</title><header><h1><a href=\"/\">daemar</a> / {}</h1></header><main>{}</main>",
        esc(&short(&slip.id)),
        esc(&short(&slip.id)),
        render_detail(slip, events)
    ))
    .into_response()
}

// ── Rendering ────────────────────────────────────────────────────────────────

fn render_board(slips: &[(Slip, Vec<ledger::Event>)]) -> String {
    let cocked: Vec<&Slip> = slips.iter().map(|(s, _)| s).filter(|s| s.cocked.is_some()).collect();
    let flying: Vec<&Slip> = slips
        .iter()
        .map(|(s, _)| s)
        .filter(|s| s.status == Status::InFlight && s.cocked.is_none())
        .collect();
    let closed: Vec<&Slip> = slips
        .iter()
        .map(|(s, _)| s)
        .filter(|s| s.status != Status::InFlight)
        .collect();

    let mut html = String::new();
    for (title, group) in [("COCKED — needs the controller", &cocked), ("IN FLIGHT", &flying), ("CLOSED", &closed)] {
        html.push_str(&format!(
            "<section><h2>{title} <span class=\"count\">{}</span></h2>",
            group.len()
        ));
        if group.is_empty() {
            html.push_str("<p class=\"empty\">— none —</p>");
        }
        for slip in group.iter() {
            html.push_str(&render_strip(slip));
        }
        html.push_str("</section>");
    }
    html
}

fn render_strip(slip: &Slip) -> String {
    let (class, badge) = match (&slip.cocked, &slip.status) {
        (Some(boundary), _) => ("cocked", format!("awaiting clearance: {}", esc(boundary))),
        (None, Status::InFlight) => (
            "inflight",
            slip.current_phase
                .as_deref()
                .map(|p| format!("flying: {}", esc(p)))
                .unwrap_or_else(|| "between phases".to_string()),
        ),
        (None, Status::Accepted) => ("accepted", "accepted".to_string()),
        (None, Status::Rejected) => ("rejected", "rejected".to_string()),
    };
    let phases: String = slip
        .phases
        .iter()
        .map(|p| {
            let dot = match p.outcome.as_deref() {
                Some("success") => "dot ok",
                Some(_) => "dot bad",
                None => "dot live",
            };
            format!("<span class=\"{dot}\" title=\"{}\"></span>", esc(&p.phase))
        })
        .collect();
    format!(
        "<a class=\"strip {class}\" href=\"/slip/{id}\">\
         <span class=\"id\">{sid}</span>\
         <span class=\"req\">{req}</span>\
         <span class=\"badge\">{badge}</span>\
         <span class=\"dots\">{phases}</span>\
         <span class=\"meta\">{wf} · {calls} calls · {tok} tok · ${cost:.2}</span>\
         </a>",
        id = esc(&slip.id),
        sid = esc(&short(&slip.id)),
        req = esc(&slip.request),
        wf = esc(&slip.workflow),
        calls = slip.model_calls,
        tok = slip.tokens,
        cost = slip.cost,
    )
}

fn render_detail(slip: &Slip, events: &[ledger::Event]) -> String {
    let mut html = String::new();

    html.push_str(&format!(
        "<div class=\"face\"><p class=\"req\">{}</p>\
         <table><tr><th>status</th><td>{:?}{}</td></tr>\
         <tr><th>workflow</th><td>{}</td></tr><tr><th>engineer</th><td>{}</td></tr>\
         <tr><th>opened</th><td>{}</td></tr><tr><th>last event</th><td>{}</td></tr>\
         <tr><th>spend</th><td>{} model calls · {} tokens · ${:.2} · {} queries</td></tr>{}</table></div>",
        esc(&slip.request),
        slip.status,
        slip.cocked.as_deref().map(|b| format!(" — COCKED at {}", esc(b))).unwrap_or_default(),
        esc(&slip.workflow),
        esc(&slip.engineer),
        esc(&slip.opened_ts),
        esc(&slip.last_ts),
        slip.model_calls,
        slip.tokens,
        slip.cost,
        slip.queries,
        slip.close_reason
            .as_deref()
            .map(|r| format!("<tr><th>close reason</th><td>{}</td></tr>", esc(r)))
            .unwrap_or_default(),
    ));

    html.push_str("<h2>phases</h2><table><tr><th>phase</th><th>owner</th><th>lane</th><th>started</th><th>ended</th><th>outcome</th></tr>");
    for p in &slip.phases {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(&p.phase),
            esc(&p.owner),
            esc(&p.lane),
            esc(&p.started),
            p.ended.as_deref().map(esc).unwrap_or_else(|| "…".to_string()),
            p.outcome.as_deref().map(esc).unwrap_or_else(|| "running".to_string()),
        ));
    }
    html.push_str("</table>");

    html.push_str("<h2>sections</h2>");
    if slip.sections.is_empty() {
        html.push_str("<p class=\"empty\">— none yet —</p>");
    } else {
        html.push_str("<table><tr><th>section</th><th>by</th><th>summary</th><th>ts</th></tr>");
        for s in &slip.sections {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&s.section),
                esc(&s.by),
                esc(&s.summary),
                esc(&s.ts)
            ));
        }
        html.push_str("</table>");
    }

    html.push_str("<h2>clearances</h2>");
    if slip.clearances.is_empty() {
        html.push_str("<p class=\"empty\">— none requested —</p>");
    } else {
        html.push_str("<table><tr><th>boundary</th><th>requested by</th><th>at</th><th>response</th></tr>");
        for c in &slip.clearances {
            let response = match &c.response {
                Some((verdict, by, ts)) => {
                    let reason = if c.reason.is_empty() { String::new() } else { format!(" — {}", esc(&c.reason)) };
                    format!("{} by {} at {}{}", esc(verdict), esc(by), esc(ts), reason)
                }
                None => "<b>PENDING — strip is cocked</b>".to_string(),
            };
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(&c.boundary),
                esc(&c.requested_by),
                esc(&c.requested_ts),
                response
            ));
        }
        html.push_str("</table>");
    }

    html.push_str(&format!("<h2>raw ledger <span class=\"count\">{} events</span></h2>", events.len()));
    html.push_str("<table class=\"ledger\"><tr><th>seq</th><th>ts</th><th>kind</th><th>payload</th></tr>");
    for e in events {
        let payload = serde_json::to_string(&e.payload).unwrap_or_default();
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td></tr>",
            e.seq,
            esc(&e.ts),
            esc(&e.kind),
            esc(&payload)
        ));
    }
    html.push_str("</table>");
    html
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn short(id: &str) -> String {
    id.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

const STYLE: &str = r#"<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>
:root{color-scheme:dark}
body{background:#0f1216;color:#d7dce2;font:14px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;margin:0;padding:0 1.2rem 3rem}
header{display:flex;align-items:baseline;gap:1rem;padding:1rem 0;border-bottom:1px solid #232a33}
h1{font-size:1.1rem;margin:0;letter-spacing:.08em;text-transform:uppercase}
h1 a{color:inherit;text-decoration:none}
.sub{color:#5c6773;font-size:.8rem}
h2{font-size:.8rem;letter-spacing:.12em;color:#8a94a0;margin:1.6rem 0 .5rem;text-transform:uppercase}
.count{color:#5c6773;font-weight:normal}
.empty{color:#3f4854;margin:.2rem 0}
.strip{display:grid;grid-template-columns:4.5rem 1fr auto;grid-template-rows:auto auto;gap:.1rem .8rem;align-items:baseline;
  background:#161b22;border-left:4px solid #3f4854;border-radius:3px;padding:.5rem .8rem;margin:.4rem 0;
  text-decoration:none;color:inherit;transition:transform .15s}
.strip:hover{background:#1b222b}
.strip .id{color:#8a94a0}
.strip .req{white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.strip .badge{font-size:.75rem;color:#8a94a0}
.strip .dots{grid-column:1}
.strip .meta{grid-column:2/4;font-size:.75rem;color:#5c6773}
.strip.cocked{border-left-color:#f59e0b;transform:rotate(-1.2deg);background:#1c1810}
.strip.cocked .badge{color:#f59e0b}
.strip.inflight{border-left-color:#38bdf8}
.strip.inflight .badge{color:#38bdf8}
.strip.accepted{border-left-color:#4ade80}
.strip.rejected{border-left-color:#f87171}
.strip.rejected .badge{color:#f87171}
.dot{display:inline-block;width:.55rem;height:.55rem;border-radius:50%;margin-right:.25rem;background:#3f4854}
.dot.ok{background:#4ade80}.dot.bad{background:#f87171}.dot.live{background:#38bdf8;animation:pulse 1.2s infinite}
@keyframes pulse{50%{opacity:.35}}
table{border-collapse:collapse;width:100%;margin:.4rem 0}
th,td{text-align:left;padding:.25rem .6rem;border-bottom:1px solid #1e242c;vertical-align:top}
th{color:#5c6773;font-weight:normal}
.face .req{font-size:1rem;margin:.8rem 0 .4rem}
.ledger code{color:#8a94a0;word-break:break-all}
a{color:#38bdf8}
</style>"#;
