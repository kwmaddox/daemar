//! The board: the controller's instrument.
//!
//! One line per strip, fixed columns — position carries the semantics, so a
//! bay scans like a table and a column compares across flights. Bays are in
//! attention order: ATTENTION first (cocked strips sorted by wait, then stale
//! strips sorted by silence), then IN FLIGHT. Closed strips leave the board —
//! a controller's bay holds only what is being worked — and live at /closed.
//!
//! Staleness is derived, never asserted: an in-flight slip whose ledger has
//! been silent past the threshold. A hung agent produces silence, not
//! redness; the board makes silence visible.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use ledger::{Slip, Status};

struct App {
    ledgers: PathBuf,
    stale_secs: u64,
}

#[tokio::main]
async fn main() {
    let ledgers = std::env::var("DAEMAR_LEDGERS").unwrap_or_else(|_| "fixtures".to_string());
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(4700);
    let stale_secs: u64 = std::env::var("DAEMAR_STALE_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(120);
    let app = Arc::new(App { ledgers: PathBuf::from(ledgers), stale_secs });

    let router = Router::new()
        .route("/", get(index))
        .route("/board", get(board_fragment))
        .route("/closed", get(closed_page))
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

fn now_epoch() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ── Pages ────────────────────────────────────────────────────────────────────

async fn index(State(app): State<Arc<App>>) -> Html<String> {
    let board = render_board(&app, &load(&app));
    Html(format!(
        "{STYLE}<title>daemar — the board</title>\
         <header><h1>daemar</h1><span class=\"sub\">the board · ledgers are truth · slips are folds</span></header>\
         <main id=\"board\">{board}</main>\
         <script>setInterval(async()=>{{const r=await fetch('/board');\
         document.getElementById('board').innerHTML=await r.text();}},2000);</script>"
    ))
}

async fn board_fragment(State(app): State<Arc<App>>) -> Html<String> {
    let board = render_board(&app, &load(&app));
    Html(board)
}

async fn closed_page(State(app): State<Arc<App>>) -> Html<String> {
    let all = load(&app);
    let now = now_epoch();
    let mut closed: Vec<&Slip> = all.iter().map(|(s, _)| s).filter(|s| s.status != Status::InFlight).collect();
    closed.sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
    let mut html = format!("{STYLE}<title>daemar — closed</title><header><h1><a href=\"/\">daemar</a> / closed</h1></header><main>");
    html.push_str(&format!("<section><h2>CLOSED <span class=\"count\">{}</span></h2>", closed.len()));
    for slip in &closed {
        html.push_str(&render_strip(slip, now, u64::MAX));
    }
    html.push_str("</section><p><a href=\"/\">← board</a></p></main>");
    Html(html)
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

// ── The board ────────────────────────────────────────────────────────────────

fn render_board(app: &App, slips: &[(Slip, Vec<ledger::Event>)]) -> String {
    let now = now_epoch();

    let mut cocked: Vec<&Slip> = Vec::new();
    let mut stale: Vec<&Slip> = Vec::new();
    let mut flying: Vec<&Slip> = Vec::new();
    let mut closed_count = 0usize;

    for (slip, _) in slips {
        match slip.status {
            Status::InFlight => {
                if slip.cocked.is_some() {
                    cocked.push(slip);
                } else if silence(slip, now) >= app.stale_secs {
                    stale.push(slip);
                } else {
                    flying.push(slip);
                }
            }
            _ => closed_count += 1,
        }
    }
    // Attention order: longest-waiting clearance first, then longest silence.
    cocked.sort_by_key(|s| std::cmp::Reverse(cocked_wait(s, now)));
    stale.sort_by_key(|s| std::cmp::Reverse(silence(s, now)));
    // Healthy traffic: most recent activity first.
    flying.sort_by_key(|s| silence(s, now));

    let mut html = String::new();
    html.push_str(&format!(
        "<section><h2>ATTENTION <span class=\"count\">{}</span></h2>",
        cocked.len() + stale.len()
    ));
    if cocked.is_empty() && stale.is_empty() {
        html.push_str("<p class=\"empty\">— clean board —</p>");
    }
    for slip in cocked.iter().chain(stale.iter()) {
        html.push_str(&render_strip(slip, now, app.stale_secs));
    }
    html.push_str("</section>");

    html.push_str(&format!("<section><h2>IN FLIGHT <span class=\"count\">{}</span></h2>", flying.len()));
    if flying.is_empty() {
        html.push_str("<p class=\"empty\">— none —</p>");
    }
    for slip in &flying {
        html.push_str(&render_strip(slip, now, app.stale_secs));
    }
    html.push_str("</section>");

    html.push_str(&format!(
        "<p class=\"closedlink\"><a href=\"/closed\">closed: {closed_count} →</a></p>"
    ));
    html
}

/// One strip, one line. Columns: id · request · workflow · route · status+age
/// · airframe · events · tokens · cost.
fn render_strip(slip: &Slip, now: u64, stale_secs: u64) -> String {
    let quiet = silence(slip, now);
    let is_stale = slip.status == Status::InFlight && slip.cocked.is_none() && quiet >= stale_secs;

    let (class, status, age) = match (&slip.cocked, &slip.status) {
        (Some(_), _) => ("cocked", "CKD", format!("⚠{}", human(cocked_wait(slip, now)))),
        (None, Status::InFlight) if is_stale => ("stale", "STL", format!("silent {}", human(quiet))),
        (None, Status::InFlight) => ("inflight", "FLY", human(quiet)),
        (None, Status::Accepted) => ("accepted", "ACC", String::new()),
        (None, Status::Rejected) => ("rejected", "REJ", String::new()),
    };

    let mut route: String = slip
        .phases
        .iter()
        .map(|p| {
            let (mark, class) = match p.outcome.as_deref() {
                Some("success") => ("✓", "ok"),
                Some(_) => ("✗", "bad"),
                None => ("●", "live"),
            };
            format!("<i class=\"{class}\">{}{mark}</i>", phase_code(&p.phase))
        })
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(boundary) = &slip.cocked {
        route.push_str(&format!(" <i class=\"pend\">→{}</i>", boundary_code(boundary)));
    }

    format!(
        "<a class=\"strip {class}\" href=\"/slip/{id}\">\
         <span class=\"id\">{sid}</span>\
         <span class=\"req\">{req}</span>\
         <span class=\"wf\">{wf}</span>\
         <span class=\"route\">{route}</span>\
         <span class=\"status\">{status} <b class=\"age\">{age}</b></span>\
         <span class=\"model\">{model}</span>\
         <span class=\"num\">{ev}ev</span>\
         <span class=\"num\">{tok}</span>\
         <span class=\"num\">${cost:.2}</span>\
         </a>",
        id = esc(&slip.id),
        sid = esc(&short(&slip.id)),
        req = esc(&slip.request),
        wf = esc(&slip.workflow.to_uppercase()),
        model = esc(&airframe(slip)),
        ev = slip.event_count,
        tok = k_tokens(slip.tokens),
        cost = slip.cost,
    )
}

// ── Derivations (display-side) ───────────────────────────────────────────────

/// Seconds since the ledger last spoke.
fn silence(slip: &Slip, now: u64) -> u64 {
    ledger::parse_ts(&slip.last_ts).map(|t| now.saturating_sub(t)).unwrap_or(0)
}

/// Seconds the unanswered clearance has been waiting.
fn cocked_wait(slip: &Slip, now: u64) -> u64 {
    slip.clearances
        .iter()
        .rev()
        .find(|c| c.response.is_none())
        .and_then(|c| ledger::parse_ts(&c.requested_ts))
        .map(|t| now.saturating_sub(t))
        .unwrap_or(0)
}

fn human(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
    } else {
        format!("{}d{}h", secs / 86400, (secs % 86400) / 3600)
    }
}

fn phase_code(phase: &str) -> String {
    match phase {
        "request" => "REQ".into(),
        "plan" => "PLN".into(),
        "build" => "BLD".into(),
        "test" => "TST".into(),
        "review" => "RVW".into(),
        "document" => "DOC".into(),
        "scout" => "SCT".into(),
        "ship" => "SHP".into(),
        other => other.chars().take(3).collect::<String>().to_uppercase(),
    }
}

fn boundary_code(boundary: &str) -> String {
    match boundary.rsplit_once("->") {
        Some((_, target)) => phase_code(target.trim()),
        None => phase_code(boundary),
    }
}

/// The airframe column: model id without the provider, clipped.
fn airframe(slip: &Slip) -> String {
    let Some(model) = &slip.last_model else { return "—".into() };
    let bare = model.rsplit('/').next().unwrap_or(model);
    bare.chars().take(12).collect()
}

fn k_tokens(tokens: u64) -> String {
    if tokens >= 1000 {
        format!("{}k", tokens / 1000)
    } else {
        tokens.to_string()
    }
}

// ── Detail page ──────────────────────────────────────────────────────────────

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
body{background:#0f1216;color:#d7dce2;font:13px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace;margin:0;padding:0 1.2rem 3rem}
header{display:flex;align-items:baseline;gap:1rem;padding:.8rem 0;border-bottom:1px solid #232a33}
h1{font-size:1rem;margin:0;letter-spacing:.08em;text-transform:uppercase}
h1 a{color:inherit;text-decoration:none}
.sub{color:#5c6773;font-size:.75rem}
h2{font-size:.72rem;letter-spacing:.14em;color:#8a94a0;margin:1.2rem 0 .3rem;text-transform:uppercase}
.count{color:#5c6773;font-weight:normal}
.empty{color:#3f4854;margin:.2rem 0}
.closedlink{margin-top:1.2rem}.closedlink a{color:#5c6773}
.strip{display:grid;grid-template-columns:3rem minmax(11rem,1fr) 3.4rem minmax(11rem,17rem) 7.6rem 7rem 3.2rem 3.2rem 3.6rem;
  gap:0 .7rem;align-items:baseline;white-space:nowrap;
  background:#161b22;border-left:3px solid #3f4854;border-radius:2px;padding:.28rem .6rem;margin:.22rem 0;
  text-decoration:none;color:inherit}
.strip:hover{background:#1b222b}
.strip .id{color:#8a94a0}
.strip .req,.strip .route{overflow:hidden;text-overflow:ellipsis}
.strip .wf{color:#8a94a0;font-size:.72rem}
.strip .status{font-size:.78rem}
.strip .model{color:#8a94a0;font-size:.75rem;overflow:hidden;text-overflow:ellipsis}
.strip .num{color:#5c6773;font-size:.75rem;text-align:right}
.route i{font-style:normal;color:#5c6773}
.route i.ok{color:#4ade80}.route i.bad{color:#f87171}
.route i.live{color:#38bdf8;animation:pulse 1.2s infinite}
.route i.pend{color:#f59e0b}
@keyframes pulse{50%{opacity:.35}}
.strip.cocked{border-left-color:#f59e0b;transform:rotate(-1.2deg);background:#1c1810}
.strip.cocked .status{color:#f59e0b}
.strip.stale{border-left-color:#f87171;opacity:.75}
.strip.stale .status{color:#f87171}
.strip.inflight{border-left-color:#38bdf8}
.strip.inflight .status{color:#38bdf8}
.strip.accepted{border-left-color:#4ade80}.strip.accepted .status{color:#4ade80}
.strip.rejected{border-left-color:#f87171}.strip.rejected .status{color:#f87171}
table{border-collapse:collapse;width:100%;margin:.4rem 0}
th,td{text-align:left;padding:.25rem .6rem;border-bottom:1px solid #1e242c;vertical-align:top}
th{color:#5c6773;font-weight:normal}
.face .req{font-size:1rem;margin:.8rem 0 .4rem;white-space:normal}
.ledger code{color:#8a94a0;word-break:break-all}
a{color:#38bdf8}
</style>"#;
