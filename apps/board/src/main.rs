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

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use ledger::{FoldedSlip, LoadReport, PhaseOutcome, Slip, Status};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt};

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
        .route("/slip/{id}", get(slip_page))
        .route("/fragment/slip/{id}", get(slip_fragment))
        .route("/events", get(events))
        .with_state(app.clone());

    let addr = format!("127.0.0.1:{port}");
    println!("[board] serving http://{addr}  ledgers: {}", app.ledgers.display());
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    axum::serve(listener, router).await.expect("serve");
}

/// Load the fleet. A directory-level failure is not swallowed — it comes
/// back as a skipped entry the board renders as a warning.
fn load(app: &App) -> LoadReport {
    match ledger::load_dir(&app.ledgers) {
        Ok(report) => report,
        Err(error) => LoadReport {
            slips: Vec::new(),
            skipped: vec![(app.ledgers.clone(), error)],
        },
    }
}

fn find<'a>(report: &'a LoadReport, id: &str) -> Option<&'a FoldedSlip> {
    report.slips.iter().find(|f| f.slip.id.0 == id)
}

fn now_epoch() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ── Change push ──────────────────────────────────────────────────────────────

/// One number that moves when any ledger moves: paths, lengths, mtimes hashed.
fn fingerprint(dir: &std::path::Path) -> u64 {
    let mut h = DefaultHasher::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        let mut entries: Vec<_> = rd
            .flatten()
            .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("jsonl"))
            .collect();
        entries.sort_by_key(|e| e.path());
        for e in entries {
            e.path().hash(&mut h);
            if let Ok(md) = e.metadata() {
                md.len().hash(&mut h);
                if let Ok(m) = md.modified() {
                    if let Ok(d) = m.duration_since(UNIX_EPOCH) {
                        d.as_nanos().hash(&mut h);
                    }
                }
            }
        }
    }
    h.finish()
}

/// SSE: the server watches the ledger dir (300ms fingerprint) and pushes
/// "change" the moment anything moves. The client refetches on push, so the
/// board reacts to a ledger write in ~300ms instead of a blind 2s poll. Files
/// stay the only truth — this is a doorbell, not a data channel.
async fn events(State(app): State<Arc<App>>) -> Sse<impl Stream<Item = Result<SseEvent, std::convert::Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<()>(8);
    let dir = app.ledgers.clone();
    tokio::spawn(async move {
        let mut last = fingerprint(&dir);
        loop {
            tokio::time::sleep(Duration::from_millis(300)).await;
            if tx.is_closed() {
                return;
            }
            let now = fingerprint(&dir);
            if now != last {
                last = now;
                if tx.send(()).await.is_err() {
                    return;
                }
            }
        }
    });
    Sse::new(ReceiverStream::new(rx).map(|_| Ok(SseEvent::default().data("change"))))
        .keep_alive(KeepAlive::default())
}

// ── Pages ────────────────────────────────────────────────────────────────────

/// The two-pane shell: bays on the left, the detail panel (and, one day, the
/// chat) docked on the right. `selected` pre-renders the panel so /slip/{id}
/// deep links work before a single line of JS runs. `closed_view` keeps the
/// left column showing the closed list when that is where the reader is —
/// a closed slip's detail must never dump you back into live traffic.
fn shell(app: &App, selected: Option<&str>, closed_view: bool) -> String {
    let report = load(app);
    let board = if closed_view { render_closed(&report) } else { render_board(app, &report) };
    let (init, detail) = match selected.and_then(|id| find(&report, id)) {
        Some(folded) => (
            folded.slip.id.0.clone(),
            render_detail(&folded.slip, &folded.events, &folded.bad_lines),
        ),
        None => (String::new(), String::new()),
    };
    let viewing = if init.is_empty() { "" } else { " class=\"viewing\"" };
    let (bays_url, home) = if closed_view { ("/board?view=closed", "/closed") } else { ("/board", "/") };
    format!(
        "{STYLE}<title>daemar — the board</title>\
         <header><h1>daemar</h1><span class=\"sub\">the board · ledgers are truth · slips are folds</span></header>\
         <main{viewing}><section id=\"bays\">{board}</section><aside id=\"detail\">{detail}</aside></main>\
         <script>\
         let sel={init:?}||null;const BAYS={bays_url:?},HOME={home:?};\
         const mainEl=document.querySelector('main');\
         function mark(){{document.querySelectorAll('a.strip').forEach(a=>a.classList.toggle('selected',a.getAttribute('href')==='/slip/'+sel));}}\
         function setViewing(){{mainEl.classList.toggle('viewing',!!sel);}}\
         function closePanel(push){{sel=null;setViewing();mark();if(push)history.pushState({{}},'',HOME);}}\
         async function loadBays(){{const r=await fetch(BAYS);document.getElementById('bays').innerHTML=await r.text();mark();}}\
         async function loadDetail(id,push){{const changed=id!==sel;sel=id;setViewing();\
           const p=document.getElementById('detail');const st=p.scrollTop;\
           const r=await fetch('/fragment/slip/'+id);if(r.ok){{p.innerHTML=await r.text();p.scrollTop=changed?0:st;}}\
           if(changed){{p.classList.remove('enter');void p.offsetWidth;p.classList.add('enter');}}\
           mark();if(push)history.pushState({{id}},'','/slip/'+id);}}\
         document.addEventListener('click',e=>{{\
           const x=e.target.closest('a.close');if(x){{e.preventDefault();closePanel(true);return;}}\
           const a=e.target.closest('a.strip');if(!a)return;e.preventDefault();\
           const id=a.getAttribute('href').split('/').pop();\
           if(id===sel){{closePanel(true);}}else{{loadDetail(id,true);}}}});\
         window.addEventListener('popstate',()=>{{const m=location.pathname.match(/^\\/slip\\/(.+)$/);\
           if(m)loadDetail(m[1],false);else closePanel(false);}});\
         new EventSource('/events').onmessage=()=>{{loadBays();if(sel)loadDetail(sel,false);}};\
         mark();setInterval(()=>{{loadBays();if(sel)loadDetail(sel,false);}},5000);\
         </script>",
        init = init,
    )
}

async fn index(State(app): State<Arc<App>>) -> Html<String> {
    Html(shell(&app, None, false))
}

async fn board_fragment(State(app): State<Arc<App>>, RawQuery(query): RawQuery) -> Html<String> {
    let report = load(&app);
    let closed_view = query.as_deref().is_some_and(|q| q.contains("view=closed"));
    Html(if closed_view { render_closed(&report) } else { render_board(&app, &report) })
}

async fn closed_page(State(app): State<Arc<App>>) -> Html<String> {
    Html(shell(&app, None, true))
}

/// Deep link: the same two-pane shell with the panel pre-loaded. A closed
/// slip restores the closed view on the left, not live traffic.
async fn slip_page(State(app): State<Arc<App>>, Path(id): Path<String>) -> Response {
    let closed_view = find(&load(&app), &id).is_some_and(|f| f.slip.status != Status::InFlight);
    Html(shell(&app, Some(id.as_str()), closed_view)).into_response()
}

/// The detail panel alone — what the board's JS swaps in.
async fn slip_fragment(State(app): State<Arc<App>>, Path(id): Path<String>) -> Response {
    let report = load(&app);
    let Some(folded) = find(&report, &id) else {
        return (StatusCode::NOT_FOUND, Html(format!("<p class=\"empty\">no slip {}</p>", esc(&id)))).into_response();
    };
    Html(render_detail(&folded.slip, &folded.events, &folded.bad_lines)).into_response()
}

// ── The board ────────────────────────────────────────────────────────────────

fn render_board(app: &App, report: &LoadReport) -> String {
    let now = now_epoch();

    let mut cocked: Vec<&Slip> = Vec::new();
    let mut stale: Vec<&Slip> = Vec::new();
    let mut flying: Vec<&Slip> = Vec::new();
    let mut closed_count = 0usize;

    for FoldedSlip { slip, .. } in &report.slips {
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
    html.push_str(&render_warnings(report));
    html
}

/// Nothing is dropped silently: unreadable files and lines are announced.
fn render_warnings(report: &LoadReport) -> String {
    let bad_lines = report.bad_line_count();
    if report.skipped.is_empty() && bad_lines == 0 {
        return String::new();
    }
    let mut parts = Vec::new();
    if bad_lines > 0 {
        parts.push(format!("{bad_lines} unreadable line(s)"));
    }
    for (path, error) in &report.skipped {
        parts.push(format!("{}: {}", path.display(), error));
    }
    format!("<p class=\"warn\">⚠ {}</p>", esc(&parts.join(" · ")))
}

/// The closed archive as a bays fragment: same strips, same panel mechanics.
fn render_closed(report: &LoadReport) -> String {
    let now = now_epoch();
    let mut closed: Vec<&Slip> = report
        .slips
        .iter()
        .map(|f| &f.slip)
        .filter(|s| s.status != Status::InFlight)
        .collect();
    closed.sort_by(|a, b| b.last_ts.cmp(&a.last_ts));
    let mut html = format!("<section><h2>CLOSED <span class=\"count\">{}</span></h2>", closed.len());
    if closed.is_empty() {
        html.push_str("<p class=\"empty\">— nothing closed yet —</p>");
    }
    for slip in &closed {
        html.push_str(&render_strip(slip, now, u64::MAX));
    }
    html.push_str("</section><p class=\"closedlink\"><a href=\"/\">← board</a></p>");
    html.push_str(&render_warnings(report));
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
            let (mark, class) = match p.outcome {
                Some(PhaseOutcome::Success) => ("✓", "ok"),
                Some(PhaseOutcome::Error) => ("✗", "bad"),
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
        id = esc(&slip.id.0),
        sid = esc(&short(&slip.id.0)),
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

fn render_detail(slip: &Slip, events: &[ledger::Event], bad_lines: &[u64]) -> String {
    let mut html = String::new();

    html.push_str(&format!(
        "<p class=\"detailid\">slip {} · <span>{}</span><a class=\"close\" href=\"/\" title=\"close\">×</a></p>",
        esc(&short(&slip.id.0)),
        esc(&slip.id.0)
    ));
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
            p.lane,
            esc(&p.started),
            p.ended.as_deref().map(esc).unwrap_or_else(|| "…".to_string()),
            p.outcome.map(|o| o.to_string()).unwrap_or_else(|| "running".to_string()),
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
                Some(r) => {
                    let reason = if r.reason.is_empty() { String::new() } else { format!(" — {}", esc(&r.reason)) };
                    format!("{} by {} at {}{}", r.verdict, esc(&r.by), esc(&r.ts), reason)
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

    let bad_note = if bad_lines.is_empty() {
        String::new()
    } else {
        format!(
            " <span class=\"warn\">⚠ {} unreadable line(s): {}</span>",
            bad_lines.len(),
            bad_lines.iter().map(u64::to_string).collect::<Vec<_>>().join(", ")
        )
    };
    html.push_str(&format!(
        "<h2>raw ledger <span class=\"count\">{} events</span>{bad_note}</h2>",
        events.len()
    ));
    html.push_str("<table class=\"ledger\"><tr><th>seq</th><th>ts</th><th>kind</th><th>payload</th></tr>");
    for e in events {
        let (kind, payload) = e.kind.wire();
        let payload = serde_json::to_string(&payload).unwrap_or_default();
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td></tr>",
            e.seq,
            esc(&e.ts),
            esc(&kind),
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
main,header{max-width:1100px;margin-inline:auto;transition:max-width .25s ease}
body:has(main.viewing) header{max-width:1720px}
main{display:grid;grid-template-columns:1fr 0fr;gap:0;align-items:start;transition:grid-template-columns .25s ease,max-width .25s ease,gap .25s ease}
main.viewing{grid-template-columns:minmax(360px,620px) minmax(560px,1fr);gap:0 1.4rem;max-width:1720px}
main.single{display:block}
#detail{position:sticky;top:0;max-height:100vh;overflow-y:auto;overflow-x:hidden;min-width:0;border-left:1px solid #232a33;padding:.4rem .2rem 2rem 1.4rem}
main:not(.viewing) #detail{border-left:none;padding:0}
.close{float:right;color:#5c6773;text-decoration:none;font-size:1rem;line-height:1;padding:0 .3rem}
.close:hover{color:#d7dce2}
#detail.enter{animation:slidein .22s ease}
@keyframes slidein{from{opacity:0;transform:translateX(14px)}to{opacity:1;transform:none}}
.detailid{color:#5c6773;font-size:.72rem;letter-spacing:.1em;text-transform:uppercase;margin:.4rem 0 0}
.detailid span{text-transform:none;letter-spacing:0}
.placeholder{margin-top:2.4rem}
.strip.selected{outline:1px solid #38bdf8;outline-offset:-1px}
header{display:flex;align-items:baseline;gap:1rem;padding:.8rem 0;border-bottom:1px solid #232a33}
h1{font-size:1rem;margin:0;letter-spacing:.08em;text-transform:uppercase}
h1 a{color:inherit;text-decoration:none}
.sub{color:#5c6773;font-size:.75rem}
h2{font-size:.72rem;letter-spacing:.14em;color:#8a94a0;margin:1.2rem 0 .3rem;text-transform:uppercase}
.count{color:#5c6773;font-weight:normal}
.empty{color:#3f4854;margin:.2rem 0}
.closedlink{margin-top:1.2rem}.closedlink a{color:#5c6773}
.warn{color:#f59e0b;font-size:.75rem}
.strip{display:grid;grid-template-columns:3rem minmax(11rem,1fr) 3.4rem minmax(11rem,17rem) 7.6rem 7rem 3.2rem 3.2rem 3.6rem;
  gap:0 .7rem;align-items:baseline;white-space:nowrap;
  background:#161b22;border-left:3px solid #3f4854;border-radius:2px;padding:.28rem .6rem;margin:.22rem 0;
  text-decoration:none;color:inherit;
  transition:background .15s,transform .2s ease,grid-template-columns .25s ease,gap .25s ease}
.strip>span{min-width:0;overflow:hidden}
.strip:hover{background:#1b222b;transform:translateX(3px)}
.strip.cocked:hover{transform:rotate(-1.2deg) translateX(3px)}
.strip .wf,.strip .model,.strip .num{transition:opacity .2s}
main.viewing .strip{grid-template-columns:2.6rem minmax(7rem,1fr) 0rem minmax(5.5rem,10rem) 8.6rem 0rem 0rem 0rem 0rem;gap:0 .35rem}
main.viewing .strip .wf,main.viewing .strip .model,main.viewing .strip .num{opacity:0}
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
