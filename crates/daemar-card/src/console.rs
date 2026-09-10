//! `card serve` console foundations (PER-84, S3-B1).

use crate::Error;
use serde::ser::{Serialize, SerializeStruct, Serializer};
use serde_json::json;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopbackIp {
    V4,
    V6,
}
/// A loopback socket address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackAddr {
    ip: LoopbackIp,
    port: Port,
}
/// A TCP port number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Port(u16);
impl Port {
    /// Requests an OS-assigned port.
    pub const EPHEMERAL: Port = Port(0);
    /// Creates a port from its wire representation.
    #[must_use]
    pub const fn new(port: u16) -> Self {
        Self(port)
    }
    #[must_use]
    /// Returns the wire representation.
    pub const fn get(self) -> u16 {
        self.0
    }
}
impl std::fmt::Display for Port {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl LoopbackAddr {
    #[must_use]
    /// Creates an IPv4 loopback address.
    pub fn v4(port: Port) -> Self {
        Self {
            ip: LoopbackIp::V4,
            port,
        }
    }
    #[must_use]
    /// Creates an IPv6 loopback address.
    pub fn v6(port: Port) -> Self {
        Self {
            ip: LoopbackIp::V6,
            port,
        }
    }
    #[must_use]
    /// Returns the port component.
    pub fn port(&self) -> Port {
        self.port
    }
    #[must_use]
    /// Returns the standard library address used for binding.
    pub fn socket_addr(&self) -> SocketAddr {
        let ip = match self.ip {
            LoopbackIp::V4 => IpAddr::V4(Ipv4Addr::LOCALHOST),
            LoopbackIp::V6 => IpAddr::V6(Ipv6Addr::LOCALHOST),
        };
        SocketAddr::new(ip, self.port.get())
    }
}
impl std::fmt::Display for LoopbackAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.socket_addr().fmt(f)
    }
}
/// The fixed port used when no port is supplied.
pub const DEFAULT_PORT: Port = Port::new(7331);
/// A bound loopback listener.
#[derive(Debug)]
pub struct Listener {
    inner: tokio::net::TcpListener,
    bound: LoopbackAddr,
}
impl Listener {
    /// Binds exactly the requested loopback address.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Bind`] when the operating system rejects the bind.
    pub async fn bind(addr: LoopbackAddr) -> Result<Self, Error> {
        let inner = tokio::net::TcpListener::bind(addr.socket_addr())
            .await
            .map_err(|source| Error::Bind { addr, source })?;
        let port = Port::new(
            inner
                .local_addr()
                .map_err(|source| Error::Bind { addr, source })?
                .port(),
        );
        Ok(Self {
            inner,
            bound: LoopbackAddr { ip: addr.ip, port },
        })
    }
    #[must_use]
    /// Returns the address actually assigned by the OS.
    pub fn bound(&self) -> LoopbackAddr {
        self.bound
    }
}
/// The startup record published before serving requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Startup {
    bound: LoopbackAddr,
    db_path: PathBuf,
    source: ConfigSource,
}
/// The source used to resolve the database path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    /// Explicit command-line flag.
    Flag,
    /// Environment variable.
    Env,
    /// Dot-env file.
    DotEnv,
    /// Built-in default.
    Default,
}
impl std::fmt::Display for ConfigSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Flag => "flag",
            Self::Env => "env",
            Self::DotEnv => "dotenv",
            Self::Default => "default",
        })
    }
}
impl Startup {
    /// Builds a startup record from the bound listener.
    #[must_use]
    pub fn from_listener(listener: &Listener, db_path: PathBuf, source: ConfigSource) -> Self {
        Self {
            bound: listener.bound(),
            db_path,
            source,
        }
    }
    #[must_use]
    /// Returns the console URL.
    pub fn url(&self) -> String {
        format!("http://{}/", self.bound)
    }
    #[must_use]
    /// Returns the bound loopback address.
    pub fn bound(&self) -> LoopbackAddr {
        self.bound
    }
    #[must_use]
    /// Returns the database path.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
    #[must_use]
    /// Returns the configuration source.
    pub fn source(&self) -> ConfigSource {
        self.source
    }
    #[must_use]
    /// Returns the startup object for JSON serialization.
    pub fn to_json(&self) -> serde_json::Value {
        json!({ "url": self.url(), "db_path": self.db_path, "source": self.source.to_string() })
    }
}

struct StartupWire<'a>(&'a Startup);

struct StartupUrl<'a>(&'a LoopbackAddr);

struct DisplayValue<T>(T);

impl<T: std::fmt::Display> Serialize for DisplayValue<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&self.0)
    }
}

impl std::fmt::Display for StartupUrl<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "http://{}/", self.0)
    }
}

impl Serialize for StartupUrl<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl Serialize for StartupWire<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut record = serializer.serialize_struct("Startup", 3)?;
        record.serialize_field("db_path", &DisplayValue(self.0.db_path.display()))?;
        record.serialize_field("source", &DisplayValue(&self.0.source))?;
        record.serialize_field("url", &StartupUrl(&self.0.bound))?;
        record.end()
    }
}
/// Writes one startup JSON line and flushes the destination.
///
/// # Errors
///
/// Returns [`Error::PublishStartup`] when writing or flushing fails.
pub fn publish_startup(out: &mut impl std::io::Write, startup: &Startup) -> Result<(), Error> {
    serde_json::to_writer(&mut *out, &StartupWire(startup)).map_err(|source| {
        Error::PublishStartup {
            source: std::io::Error::other(source),
        }
    })?;
    std::io::Write::write_all(out, b"\n").map_err(|source| Error::PublishStartup { source })?;
    out.flush()
        .map_err(|source| Error::PublishStartup { source })
}

mod web {
    use super::{Listener, LoopbackAddr};
    use crate::{CardId, Entry, EntryId, Error, Reader};
    use askama::Template;
    use axum::{
        body::Body,
        extract::{Path, Query, State},
        http::{header, Method, Request, StatusCode},
        middleware::{self, Next},
        response::Response,
        routing::get,
        Router,
    };
    use hyper_util::{
        rt::{TokioExecutor, TokioIo},
        server::conn::auto::Builder,
        service::TowerToHyperService,
    };
    use std::{collections::HashMap, sync::Arc};

    #[derive(Template)]
    #[template(path = "queue.html")]
    struct QueueTemplate {
        cards: Vec<CardView>,
        content: Option<CardView>,
    }
    #[derive(Template)]
    #[template(path = "error.html")]
    struct ErrorTemplate {
        cards: Vec<CardView>,
        message: String,
    }
    #[derive(Clone)]
    struct CardView {
        id: CardId,
        title: String,
        task_key: Option<String>,
        workspace: Option<String>,
        created_at: String,
        last_activity: String,
        entries: Vec<EntryView>,
        inspector: Option<EntryView>,
        selected: bool,
    }
    #[derive(Clone)]
    struct EntryView {
        entry_id: EntryId,
        card_id: CardId,
        sequence: u64,
        schema_version: u32,
        entry_type: crate::EntryType,
        // ast-grep-ignore: no-stringly-typed-field -- producer identity is opaque durable text in the domain
        producer_id: String,
        producer_kind: crate::ProducerKind,
        recorded_at: String,
        summary: String,
        reason: Option<String>,
        stage: Option<String>,
        payload: Option<String>,
    }

    fn timestamp(value: time::OffsetDateTime) -> String {
        value
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| value.to_string())
    }
    fn entry_view(entry: &Entry) -> Result<EntryView, Error> {
        let fields = entry.payload.history_fields()?;
        let summary = fields
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                fields
                    .get("payload")
                    .and_then(|p| p.get("summary"))
                    .and_then(serde_json::Value::as_str)
            })
            .or_else(|| {
                fields
                    .get("payload")
                    .and_then(|p| p.get("title"))
                    .and_then(serde_json::Value::as_str)
            })
            .unwrap_or("")
            .to_owned();
        let reason = fields
            .get("payload")
            .and_then(|p| p.get("reason"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let stage = fields
            .get("stage")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let payload = fields.get("payload").map(serde_json::Value::to_string);
        Ok(EntryView {
            entry_id: entry.entry_id.clone(),
            card_id: entry.card_id.clone(),
            sequence: entry.sequence,
            schema_version: entry.payload.schema_version(),
            entry_type: entry.payload.entry_type(),
            producer_id: entry.producer.id.clone(),
            producer_kind: entry.producer.kind,
            recorded_at: timestamp(entry.recorded_at),
            summary,
            reason,
            stage,
            payload,
        })
    }
    async fn queue(reader: &Reader) -> Result<Vec<CardView>, Error> {
        reader
            .queue()
            .await?
            .into_iter()
            .map(|q| {
                Ok(CardView {
                    id: q.card.card_id,
                    title: q.card.title,
                    task_key: q.card.task_key,
                    workspace: q.card.workspace,
                    created_at: timestamp(q.card.created_at),
                    last_activity: timestamp(q.last_activity),
                    entries: Vec::new(),
                    inspector: None,
                    selected: false,
                })
            })
            .collect()
    }
    fn rendered<T: Template>(template: T, status: StatusCode) -> Response {
        rendered_result(template.render(), status)
    }

    fn rendered_result(result: Result<String, askama::Error>, status: StatusCode) -> Response {
        match result {
            Ok(body) => Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Body::from(body))
                .unwrap_or_else(|_| Response::new(Body::empty())),
            Err(_) => Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(Body::from("console rendering failed"))
                .unwrap_or_else(|_| Response::new(Body::empty())),
        }
    }
    fn error_page(
        cards: Vec<CardView>,
        status: StatusCode,
        message: impl Into<String>,
    ) -> Response {
        rendered(
            ErrorTemplate {
                cards,
                message: message.into(),
            },
            status,
        )
    }

    async fn host_guard(
        State(bound): State<LoopbackAddr>,
        request: Request<Body>,
        next: Next,
    ) -> Response {
        let accepted = [
            format!("127.0.0.1:{}", bound.port()),
            format!("localhost:{}", bound.port()),
            format!("[::1]:{}", bound.port()),
        ];
        let host = request
            .headers()
            .get(header::HOST)
            .and_then(|h| h.to_str().ok());
        if host.is_none() || !accepted.iter().any(|v| Some(v.as_str()) == host) {
            return Response::builder()
                .status(StatusCode::MISDIRECTED_REQUEST)
                .body(Body::empty())
                .unwrap_or_else(|_| Response::new(Body::empty()));
        }
        next.run(request).await
    }
    async fn method_guard(request: Request<Body>, next: Next) -> Response {
        if request.method() != Method::GET && request.method() != Method::HEAD {
            return Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .header(header::ALLOW, "GET, HEAD")
                .body(Body::empty())
                .unwrap_or_else(|_| Response::new(Body::empty()));
        }
        next.run(request).await
    }
    async fn stylesheet() -> Response {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/css; charset=utf-8")
            .body(Body::from(include_str!("../static/console.css")))
            .unwrap_or_else(|_| Response::new(Body::empty()))
    }
    async fn index(State(reader): State<Arc<Reader>>) -> Response {
        match queue(&reader).await {
            Ok(cards) => rendered(
                QueueTemplate {
                    cards,
                    content: None,
                },
                StatusCode::OK,
            ),
            Err(_) => error_page(
                Vec::new(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage failed",
            ),
        }
    }
    async fn card_page(
        State(reader): State<Arc<Reader>>,
        Path(id): Path<String>,
        Query(query): Query<HashMap<String, String>>,
    ) -> Response {
        let Ok(mut cards) = queue(&reader).await else {
            return error_page(
                Vec::new(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "storage failed",
            );
        };
        let card_id = CardId::from(id);
        let Some(mut card) = cards.iter().find(|c| c.id == card_id).cloned() else {
            return error_page(cards, StatusCode::NOT_FOUND, "Card not found");
        };
        let history = match reader.history(&card_id, None).await {
            Ok(v) => v,
            Err(Error::CardNotFound { .. }) => {
                return error_page(cards, StatusCode::NOT_FOUND, "Card not found")
            }
            Err(_) => {
                return error_page(cards, StatusCode::INTERNAL_SERVER_ERROR, "storage failed")
            }
        };
        card.entries = match history.iter().map(entry_view).collect() {
            Ok(v) => v,
            Err(_) => {
                return error_page(cards, StatusCode::INTERNAL_SERVER_ERROR, "storage failed")
            }
        };
        if let Some(entry_id) = query.get("entry") {
            let eid = EntryId::from(entry_id.clone());
            match reader.entry(&card_id, &eid).await {
                Ok(entry) => card.inspector = entry_view(&entry).ok(),
                Err(Error::EntryNotFound { .. } | Error::CardNotFound { .. }) => {
                    return error_page(cards, StatusCode::NOT_FOUND, "entry not found")
                }
                Err(_) => {
                    return error_page(cards, StatusCode::INTERNAL_SERVER_ERROR, "storage failed")
                }
            }
        }
        for item in &mut cards {
            item.selected = item.id == card.id;
        }
        rendered(
            QueueTemplate {
                cards,
                content: Some(card),
            },
            StatusCode::OK,
        )
    }
    fn router(reader: Arc<Reader>, bound: LoopbackAddr) -> Router {
        Router::new()
            .route("/", get(index))
            .route("/cards/{id}", get(card_page))
            .route("/static/console.css", get(stylesheet))
            .with_state(reader)
            .layer(middleware::from_fn(method_guard))
            .layer(middleware::from_fn_with_state(bound, host_guard))
    }
    async fn serve_with_accept<F>(
        listener: Listener,
        reader: Reader,
        mut accept: F,
    ) -> Result<(), Error>
    where
        F: for<'a> FnMut(
            &'a tokio::net::TcpListener,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = std::io::Result<(tokio::net::TcpStream, std::net::SocketAddr)>,
                    > + 'a,
            >,
        >,
    {
        let bound = listener.bound();
        // Axum requires Clone state so it can share the read-only Reader across
        // concurrent request tasks; sharing this capability carries no mutation.
        let app = router(Arc::new(reader), bound);
        let builder = Builder::new(TokioExecutor::new());
        loop {
            let (stream, _) = accept(&listener.inner)
                .await
                .map_err(|source| Error::Serve { source })?;
            let io = TokioIo::new(stream);
            let service = TowerToHyperService::new(app.clone());
            let connection_builder = builder.clone();
            tokio::spawn(async move {
                let _ = connection_builder.serve_connection(io, service).await;
            });
        }
    }
    pub(super) async fn serve(listener: Listener, reader: Reader) -> Result<(), Error> {
        serve_with_accept(listener, reader, |listener| Box::pin(listener.accept())).await
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::console::Port;
        use crate::{AppendEntry, CardId, CreateCard, EntryId, Payload, Producer, ProducerKind};
        use axum::{body::to_bytes, http::Request};
        use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};
        use sqlx::ConnectOptions as _;
        use std::{error::Error as _, io, path::Path};
        use tower::ServiceExt;

        // Router tests use a deterministic address but never bind it.
        const ROUTER_FIXTURE_PORT: u16 = 7331;

        fn fixture_host(port: u16) -> String {
            format!("127.0.0.1:{port}")
        }

        fn foreign_fixture_host() -> String {
            format!("evil.test:{ROUTER_FIXTURE_PORT}")
        }

        struct Fixture {
            _dir: tempfile::TempDir,
            path: std::path::PathBuf,
            reader: Arc<Reader>,
            card_id: CardId,
            no_payload_entry: EntryId,
            payload_entry: EntryId,
        }

        async fn basic_reader() -> (tempfile::TempDir, Reader) {
            let dir = tempfile::TempDir::new().expect("temp dir");
            let path = dir.path().join("cards.db");
            let store = crate::Store::open(&path).await.expect("store");
            drop(store);
            let reader = Reader::open_existing(&path).await.expect("reader");
            (dir, reader)
        }

        async fn fixture() -> Fixture {
            let dir = tempfile::TempDir::new().expect("temp dir");
            let path = dir.path().join("cards.db");
            let store = crate::Store::open(&path).await.expect("store");
            let card_id = store
                .create_card(CreateCard {
                    title: "HTTP fixture Card".to_owned(),
                    task_key: None,
                    workspace: None,
                    producer: Producer {
                        id: "fixture-operator".to_owned(),
                        kind: ProducerKind::Operator,
                    },
                    idempotency_key: None,
                })
                .await
                .expect("create Card");
            let no_payload_entry = store
                .append(AppendEntry {
                    card_id: card_id.clone(),
                    payload: Payload::stage_event_from_parts(
                        crate::CURRENT_SCHEMA_VERSION,
                        "fixture-stage".to_owned(),
                        "fixture summary without payload".to_owned(),
                        None,
                    )
                    .expect("stage event"),
                    producer: Producer {
                        id: "fixture-agent".to_owned(),
                        kind: ProducerKind::Agent,
                    },
                    idempotency_key: None,
                })
                .await
                .expect("append stage event")
                .entry_id;
            let payload_entry = store
                .append(AppendEntry {
                    card_id: card_id.clone(),
                    payload: Payload::stage_event_from_parts(
                        crate::CURRENT_SCHEMA_VERSION,
                        "fixture-stage".to_owned(),
                        "fixture summary with payload".to_owned(),
                        Some(r#"{"marker":"payload-independent-value"}"#),
                    )
                    .expect("stage event with payload"),
                    producer: Producer {
                        id: "fixture-agent".to_owned(),
                        kind: ProducerKind::Agent,
                    },
                    idempotency_key: None,
                })
                .await
                .expect("append stage event with payload")
                .entry_id;
            drop(store);
            let reader = Arc::new(Reader::open_existing(&path).await.expect("reader"));
            Fixture {
                _dir: dir,
                path,
                reader,
                card_id,
                no_payload_entry,
                payload_entry,
            }
        }

        async fn response_with(
            reader: Arc<Reader>,
            method: Method,
            uri: &str,
            host: Option<&str>,
        ) -> axum::response::Response {
            let bound = LoopbackAddr::v4(Port::new(ROUTER_FIXTURE_PORT));
            let mut request = Request::builder().method(method).uri(uri);
            if let Some(host) = host {
                request = request.header(header::HOST, host);
            }
            router(reader, bound)
                .oneshot(request.body(Body::empty()).expect("request"))
                .await
                .expect("response")
        }

        async fn response(
            method: Method,
            uri: &str,
            host: Option<&str>,
        ) -> axum::response::Response {
            let fixture = fixture().await;
            response_with(fixture.reader, method, uri, host).await
        }

        async fn body(response: axum::response::Response) -> Vec<u8> {
            to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body")
                .to_vec()
        }

        async fn writable(path: &Path) -> SqliteConnection {
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false)
                .connect()
                .await
                .expect("writable fixture connection")
        }

        #[tokio::test]
        async fn foreign_host_precedes_unsafe_method() {
            let foreign = foreign_fixture_host();
            let result = response(Method::POST, "/", Some(&foreign)).await;
            assert_eq!(result.status(), StatusCode::MISDIRECTED_REQUEST);
            assert!(body(result).await.is_empty());
        }

        #[tokio::test]
        async fn empty_host_returns_empty_421() {
            let result = response(Method::GET, "/", None).await;
            assert_eq!(result.status(), StatusCode::MISDIRECTED_REQUEST);
            assert!(body(result).await.is_empty());
        }

        #[tokio::test]
        async fn host_rejection_precedes_unreadable_storage() {
            let fixture = fixture().await;
            let mut connection = writable(&fixture.path).await;
            sqlx::query("UPDATE cards SET created_at = 'not-a-timestamp'")
                .execute(&mut connection)
                .await
                .expect("corrupt queue timestamp");
            let accepted = response_with(
                fixture.reader.clone(),
                Method::GET,
                "/",
                Some(&fixture_host(ROUTER_FIXTURE_PORT)),
            )
            .await;
            assert_eq!(accepted.status(), StatusCode::INTERNAL_SERVER_ERROR);
            let foreign =
                response_with(fixture.reader, Method::GET, "/", Some("127.0.0.1:7332")).await;
            assert_eq!(foreign.status(), StatusCode::MISDIRECTED_REQUEST);
            assert!(body(foreign).await.is_empty());
        }

        #[tokio::test]
        async fn unsafe_method_unknown_route_returns_405() {
            let result = response(
                Method::POST,
                "/unknown",
                Some(&fixture_host(ROUTER_FIXTURE_PORT)),
            )
            .await;
            assert_eq!(result.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_eq!(result.headers()[header::ALLOW], "GET, HEAD");
            assert!(body(result).await.is_empty());
        }

        #[tokio::test]
        async fn unsafe_method_css_returns_405() {
            let result = response(
                Method::PUT,
                "/static/console.css",
                Some(&fixture_host(ROUTER_FIXTURE_PORT)),
            )
            .await;
            assert_eq!(result.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_eq!(result.headers()[header::ALLOW], "GET, HEAD");
            assert!(body(result).await.is_empty());
        }

        #[tokio::test]
        async fn unsafe_method_root_returns_405_with_allow_and_empty_body() {
            let result =
                response(Method::POST, "/", Some(&fixture_host(ROUTER_FIXTURE_PORT))).await;
            assert_eq!(result.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_eq!(result.headers()[header::ALLOW], "GET, HEAD");
            assert!(body(result).await.is_empty());
        }

        #[tokio::test]
        async fn unsafe_method_card_returns_405_with_allow_and_empty_body() {
            let fixture = fixture().await;
            let uri = format!("/cards/{}", fixture.card_id);
            let result = response_with(
                fixture.reader,
                Method::PUT,
                &uri,
                Some(&fixture_host(ROUTER_FIXTURE_PORT)),
            )
            .await;
            assert_eq!(result.status(), StatusCode::METHOD_NOT_ALLOWED);
            assert_eq!(result.headers()[header::ALLOW], "GET, HEAD");
            assert!(body(result).await.is_empty());
        }

        #[tokio::test]
        async fn accept_failure_returns_serve_with_source() {
            let (_dir, reader) = basic_reader().await;
            let listener = Listener::bind(LoopbackAddr::v4(Port::EPHEMERAL))
                .await
                .expect("listener");
            let result = serve_with_accept(listener, reader, |_| {
                Box::pin(async {
                    Err(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        "accept sentinel",
                    ))
                })
            })
            .await;
            let error = result.expect_err("accept failure");
            assert_eq!(error.to_string(), "console server failed");
            assert_eq!(
                error.source().expect("source").to_string(),
                "accept sentinel"
            );
        }

        #[tokio::test]
        async fn render_failure_returns_fixed_plain_text_500() {
            let result = rendered_result(
                Err(askama::Error::from(io::Error::other("render sentinel"))),
                StatusCode::OK,
            );
            assert_eq!(result.status(), StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(
                result.headers()[header::CONTENT_TYPE],
                "text/plain; charset=utf-8"
            );
            assert_eq!(body(result).await, b"console rendering failed");
        }

        #[tokio::test]
        async fn entryless_card_queue_failure_has_no_queue_or_card() {
            let fixture = fixture().await;
            let host = fixture_host(ROUTER_FIXTURE_PORT);
            let mut connection = writable(&fixture.path).await;
            sqlx::query("DELETE FROM card_entries")
                .execute(&mut connection)
                .await
                .expect("remove Card entries");

            let root = response_with(fixture.reader.clone(), Method::GET, "/", Some(&host)).await;
            assert_eq!(root.status(), StatusCode::INTERNAL_SERVER_ERROR);
            let card_uri = format!("/cards/{}", fixture.card_id);
            let card = response_with(fixture.reader, Method::GET, &card_uri, Some(&host)).await;
            assert_eq!(card.status(), StatusCode::INTERNAL_SERVER_ERROR);
            let card_body = body(card).await;
            assert!(card_body
                .windows("storage failed".len())
                .any(|w| w == b"storage failed"));
            for marker in [
                "data-role=\"queue\"",
                "data-role=\"card-identity\"",
                "data-role=\"stream\"",
                "data-role=\"inspector\"",
                "data-role=\"payload\"",
                "HTTP fixture Card",
            ] {
                assert!(!card_body
                    .windows(marker.len())
                    .any(|w| w == marker.as_bytes()));
            }
        }

        #[tokio::test]
        async fn queue_failure_has_no_queue_or_card() {
            let fixture = fixture().await;
            let mut connection = writable(&fixture.path).await;
            sqlx::query("UPDATE cards SET created_at = 'not-a-timestamp'")
                .execute(&mut connection)
                .await
                .expect("corrupt queue timestamp");
            let result = response_with(
                fixture.reader,
                Method::GET,
                "/",
                Some(&fixture_host(ROUTER_FIXTURE_PORT)),
            )
            .await;
            assert_eq!(result.status(), StatusCode::INTERNAL_SERVER_ERROR);
            let body = body(result).await;
            assert!(body
                .windows("storage failed".len())
                .any(|w| w == b"storage failed"));
            for marker in [
                "data-role=\"queue\"",
                "data-role=\"queue-row\"",
                "data-role=\"card-identity\"",
                "data-role=\"stream\"",
                "data-role=\"inspector\"",
                "data-role=\"payload\"",
                "HTTP fixture Card",
            ] {
                assert!(
                    !body.windows(marker.len()).any(|w| w == marker.as_bytes()),
                    "unexpected marker {marker}"
                );
            }
        }

        #[tokio::test]
        async fn head_matches_get_status_and_type_without_body() {
            let fixture = fixture().await;
            let card_uri = format!("/cards/{}?entry={}", fixture.card_id, fixture.payload_entry);
            for uri in [
                "/".to_owned(),
                card_uri,
                "/static/console.css".to_owned(),
                "/missing".to_owned(),
                "/cards/missing-card".to_owned(),
            ] {
                let get = response_with(
                    fixture.reader.clone(),
                    Method::GET,
                    &uri,
                    Some(&fixture_host(ROUTER_FIXTURE_PORT)),
                )
                .await;
                let expected_status = get.status();
                let expected_type = get.headers().get(header::CONTENT_TYPE).cloned();
                let get_body = body(get).await;
                let head = response_with(
                    fixture.reader.clone(),
                    Method::HEAD,
                    &uri,
                    Some(&fixture_host(ROUTER_FIXTURE_PORT)),
                )
                .await;
                assert_eq!(head.status(), expected_status, "HEAD status for {uri}");
                assert_eq!(
                    head.headers().get(header::CONTENT_TYPE),
                    expected_type.as_ref(),
                    "HEAD type for {uri}"
                );
                assert!(body(head).await.is_empty(), "HEAD body for {uri}");
                if uri == "/" || uri.starts_with("/cards/") || uri == "/static/console.css" {
                    assert!(!get_body.is_empty(), "GET control body for {uri}");
                }
            }
        }

        #[tokio::test]
        async fn payload_absence_is_rendered_without_payload_element() {
            let fixture = fixture().await;
            let absent_uri = format!(
                "/cards/{}?entry={}",
                fixture.card_id, fixture.no_payload_entry
            );
            let absent = response_with(
                fixture.reader.clone(),
                Method::GET,
                &absent_uri,
                Some(&fixture_host(ROUTER_FIXTURE_PORT)),
            )
            .await;
            assert_eq!(absent.status(), StatusCode::OK);
            let absent_body = body(absent).await;
            for marker in [
                "fixture-stage",
                "fixture summary without payload",
                "data-role=\"inspector\"",
            ] {
                assert!(
                    absent_body
                        .windows(marker.len())
                        .any(|w| w == marker.as_bytes()),
                    "missing marker {marker}"
                );
            }
            assert!(!absent_body
                .windows("data-role=\"payload\"".len())
                .any(|w| w == b"data-role=\"payload\""));

            let present_uri = format!("/cards/{}?entry={}", fixture.card_id, fixture.payload_entry);
            let present = response_with(
                fixture.reader,
                Method::GET,
                &present_uri,
                Some(&fixture_host(ROUTER_FIXTURE_PORT)),
            )
            .await;
            assert_eq!(present.status(), StatusCode::OK);
            let present_body = body(present).await;
            assert!(present_body
                .windows("data-role=\"payload\"".len())
                .any(|w| w == b"data-role=\"payload\""));
            assert!(present_body
                .windows("payload-independent-value".len())
                .any(|w| w == b"payload-independent-value"));
        }

        #[tokio::test]
        async fn stream_failure_has_queue_without_partial_card() {
            let fixture = fixture().await;
            let control = response_with(
                fixture.reader.clone(),
                Method::GET,
                "/",
                Some(&fixture_host(ROUTER_FIXTURE_PORT)),
            )
            .await;
            assert_eq!(control.status(), StatusCode::OK);
            let mut connection = writable(&fixture.path).await;
            sqlx::query("UPDATE card_entries SET payload = 'not-json' WHERE entry_id = ?1")
                .bind(fixture.payload_entry.to_string())
                .execute(&mut connection)
                .await
                .expect("corrupt stream payload");
            let uri = format!("/cards/{}", fixture.card_id);
            let result = response_with(
                fixture.reader,
                Method::GET,
                &uri,
                Some(&fixture_host(ROUTER_FIXTURE_PORT)),
            )
            .await;
            assert_eq!(result.status(), StatusCode::INTERNAL_SERVER_ERROR);
            let body = body(result).await;
            for marker in ["data-role=\"queue\"", "HTTP fixture Card", "storage failed"] {
                assert!(
                    body.windows(marker.len()).any(|w| w == marker.as_bytes()),
                    "missing marker {marker}"
                );
            }
            for marker in [
                "data-role=\"card-identity\"",
                "data-role=\"stream\"",
                "data-role=\"inspector\"",
                "data-role=\"payload\"",
            ] {
                assert!(
                    !body.windows(marker.len()).any(|w| w == marker.as_bytes()),
                    "unexpected marker {marker}"
                );
            }
        }
    }
}

/// Serves the read-only HTTP console until its listener accept loop fails.
///
/// # Errors
///
/// Returns [`Error::Serve`] when accepting a new TCP connection fails.
pub async fn serve(listener: Listener, reader: crate::Reader) -> Result<(), Error> {
    web::serve(listener, reader).await
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;
    use std::io::{self, Write};
    use std::path::PathBuf;

    use serde_json::json;

    use super::{publish_startup, ConfigSource, Listener, LoopbackAddr, Port, Startup};
    use crate::{Error, ErrorCategory};

    // Stable port used only to make address and default-port display fixtures explicit.
    const DISPLAY_FIXTURE_PORT: u16 = 7331;

    #[tokio::test]
    async fn listener_zero_reports_bound_port() {
        let requested = LoopbackAddr::v4(Port::EPHEMERAL);
        let listener = Listener::bind(requested)
            .await
            .expect("an ephemeral loopback listener should bind");

        assert_ne!(listener.bound().port(), Port::EPHEMERAL);
    }

    #[tokio::test]
    async fn occupied_port_returns_bind_error_without_retry() {
        let first = Listener::bind(LoopbackAddr::v4(Port::EPHEMERAL))
            .await
            .expect("the first loopback listener should bind");
        let requested = first.bound();
        let error = Listener::bind(requested)
            .await
            .expect_err("an occupied port must be rejected");

        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert!(matches!(error, Error::Bind { addr, .. } if addr == requested));
    }

    #[tokio::test]
    async fn startup_uses_bound_listener() {
        let listener = Listener::bind(LoopbackAddr::v4(Port::EPHEMERAL))
            .await
            .expect("the loopback listener should bind");
        let startup = Startup::from_listener(
            &listener,
            PathBuf::from("/var/lib/daemar/cards.db"),
            ConfigSource::Env,
        );
        let bound = listener.bound();

        assert_eq!(startup.bound(), bound);
        assert_eq!(startup.url(), format!("http://{bound}/"));
        assert_eq!(startup.db_path(), PathBuf::from("/var/lib/daemar/cards.db"));
        assert_eq!(startup.source(), ConfigSource::Env);
        assert_eq!(
            startup.to_json(),
            json!({
                "url": format!("http://{bound}/"),
                "db_path": "/var/lib/daemar/cards.db",
                "source": "env",
            })
        );
    }

    #[test]
    fn loopback_address_formats_both_families() {
        assert_eq!(
            LoopbackAddr::v4(Port::new(DISPLAY_FIXTURE_PORT)).to_string(),
            "127.0.0.1:7331"
        );
        assert_eq!(
            LoopbackAddr::v6(Port::new(DISPLAY_FIXTURE_PORT)).to_string(),
            "[::1]:7331"
        );
        assert_eq!(Port::new(DISPLAY_FIXTURE_PORT).get(), DISPLAY_FIXTURE_PORT);
        assert_eq!(super::DEFAULT_PORT.get(), DISPLAY_FIXTURE_PORT);
    }

    #[test]
    fn config_source_has_contract_spellings() {
        assert_eq!(ConfigSource::Flag.to_string(), "flag");
        assert_eq!(ConfigSource::Env.to_string(), "env");
        assert_eq!(ConfigSource::DotEnv.to_string(), "dotenv");
        assert_eq!(ConfigSource::Default.to_string(), "default");
    }

    #[derive(Debug)]
    struct RecordingWriter {
        bytes: Vec<u8>,
        max_per_write: usize,
        flushes: usize,
        write_error: Option<io::ErrorKind>,
        flush_error: Option<io::ErrorKind>,
    }

    impl RecordingWriter {
        fn complete() -> Self {
            Self {
                bytes: Vec::new(),
                max_per_write: usize::MAX,
                flushes: 0,
                write_error: None,
                flush_error: None,
            }
        }
    }

    impl Write for RecordingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some(kind) = self.write_error {
                return Err(io::Error::new(kind, "write sentinel"));
            }
            let count = bytes.len().min(self.max_per_write);
            self.bytes.extend_from_slice(
                bytes
                    .get(..count)
                    .expect("count is bounded by bytes length"),
            );
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if let Some(kind) = self.flush_error {
                return Err(io::Error::new(kind, "flush sentinel"));
            }
            Ok(())
        }
    }

    async fn startup_for_publication() -> Startup {
        let listener = Listener::bind(LoopbackAddr::v4(Port::EPHEMERAL))
            .await
            .expect("the loopback listener should bind");
        Startup::from_listener(&listener, PathBuf::from("cards.db"), ConfigSource::Default)
    }

    #[tokio::test]
    async fn publish_writes_one_line_and_flushes() {
        let startup = startup_for_publication().await;
        let expected = serde_json::to_string(&startup.to_json()).expect("startup JSON");
        let mut out = RecordingWriter::complete();

        publish_startup(&mut out, &startup).expect("startup publication should succeed");

        assert_eq!(out.bytes, format!("{expected}\n").as_bytes());
        assert!(out.bytes.ends_with(b"\n"));
        assert_eq!(out.flushes, 1);
    }

    #[tokio::test]
    async fn publish_handles_partial_writes() {
        let startup = startup_for_publication().await;
        let expected = format!("{}\n", serde_json::to_string(&startup.to_json()).unwrap());
        let mut out = RecordingWriter {
            max_per_write: 2,
            ..RecordingWriter::complete()
        };

        publish_startup(&mut out, &startup).expect("partial writes must be completed");

        assert_eq!(out.bytes, expected.as_bytes());
        assert_eq!(out.flushes, 1);
    }

    #[tokio::test]
    async fn publish_write_failure_preserves_source() {
        let startup = startup_for_publication().await;
        let mut out = RecordingWriter {
            write_error: Some(io::ErrorKind::BrokenPipe),
            ..RecordingWriter::complete()
        };

        let error = publish_startup(&mut out, &startup).expect_err("write failure must return");
        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(error.to_string(), "startup line could not be published");
        assert_eq!(error.source().unwrap().to_string(), "write sentinel");
        assert_eq!(out.flushes, 0);
    }

    #[tokio::test]
    async fn publish_flush_failure_preserves_source() {
        let startup = startup_for_publication().await;
        let expected = format!("{}\n", serde_json::to_string(&startup.to_json()).unwrap());
        let mut out = RecordingWriter {
            flush_error: Some(io::ErrorKind::TimedOut),
            ..RecordingWriter::complete()
        };

        let error = publish_startup(&mut out, &startup).expect_err("flush failure must return");
        assert_eq!(out.bytes, expected.as_bytes());
        assert_eq!(out.flushes, 1);
        assert_eq!(error.source().unwrap().to_string(), "flush sentinel");
    }

    #[test]
    fn publish_startup_error_category_and_display() {
        let source = io::Error::other("category sentinel");
        let error = Error::PublishStartup { source };
        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(error.to_string(), "startup line could not be published");
        assert_eq!(error.source().unwrap().to_string(), "category sentinel");
    }
}
