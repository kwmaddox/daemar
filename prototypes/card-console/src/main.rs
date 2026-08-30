use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use std::net::SocketAddr;

// The fixed port keeps the prototype URL stable for rapid visual comparison.
const LISTEN_PORT: u16 = 4187;

#[tokio::main]
async fn main() {
    let listen_address = SocketAddr::from(([127, 0, 0, 1], LISTEN_PORT));
    let app = Router::new()
        .route("/prototype/card-console", get(index))
        .route("/prototype/card-console/stop", post(stop))
        .route("/prototype/card-console/trace-detail", get(trace_detail))
        .route("/assets/prototype.css", get(styles))
        .route("/assets/prototype.js", get(script))
        .route("/assets/htmx.min.js", get(htmx));

    let listener = match tokio::net::TcpListener::bind(listen_address).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("card-console prototype could not bind {listen_address}: {error}");
            return;
        }
    };

    println!("Card console prototype: http://{listen_address}/prototype/card-console");
    if let Err(error) = axum::serve(listener, app).await {
        eprintln!("card-console prototype stopped: {error}");
    }
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn stop() -> Html<&'static str> {
    Html(
        r#"<button class="stop-button requested" type="button" disabled>
            <span class="status-dot"></span> Stop requested
        </button>"#,
    )
}

async fn trace_detail() -> Html<&'static str> {
    Html(
        r#"<div class="trace-expansion">
            <div><span>command</span><code>cargo test --workspace</code></div>
            <div><span>exit</span><code>101</code></div>
            <div><span>duration</span><code>12.4s</code></div>
            <pre>assertion failed: expected stage outcome `accepted`
  left: `running`
 right: `accepted`</pre>
            <p class="source-line"><b>TRACE OBSERVATION</b> · trace seq 184 · retained detail</p>
        </div>"#,
    )
}

async fn styles() -> Response {
    static_asset(
        "text/css; charset=utf-8",
        include_str!("../static/prototype.css"),
    )
}

async fn script() -> Response {
    static_asset(
        "text/javascript; charset=utf-8",
        include_str!("../static/prototype.js"),
    )
}

async fn htmx() -> Response {
    static_asset(
        "text/javascript; charset=utf-8",
        include_str!("../static/htmx.min.js"),
    )
}

fn static_asset(content_type: &'static str, body: &'static str) -> Response {
    let mut response = (StatusCode::OK, body).into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
}
