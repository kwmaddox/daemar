//! The console under test as a process, and a deliberately raw HTTP/1.1
//! client for driving it. Raw on purpose: the S3-B2 authority scenarios
//! send Host values a client library would refuse to emit (an empty
//! field, a foreign authority), and S3-B8 sends unsafe methods to every
//! route — the oracle must control every byte on the wire.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

use crate::{Run, CARD_BIN};

/// How long a startup line may take to appear before the scenario fails.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
/// Per-socket read/connect timeout; every request is local.
const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);

/// One HTTP response, parsed just far enough to assert on.
#[derive(Debug, Clone)]
pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: String,
    /// The request target that produced this response, for messages.
    pub(crate) target: String,
}

impl Response {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn describe(&self) -> String {
        format!(
            "{} -> {}\n{}",
            self.target,
            self.status,
            self.body.chars().take(2000).collect::<String>()
        )
    }
}

/// A running `card serve` process: killed on drop so a failed scenario
/// never leaks a listener into the next one.
#[derive(Debug)]
pub(crate) struct Console {
    child: Child,
    /// The parsed startup line (S3-B1): `url`, `db_path`, `source`.
    pub(crate) startup: Value,
    pub(crate) host: String,
    pub(crate) port: u16,
}

impl Drop for Console {
    fn drop(&mut self) {
        // Best effort: a process that already exited returns an error
        // here, and that is fine.
        let _killed = self.child.kill();
        let _reaped = self.child.wait();
    }
}

/// The outcome of one `card serve` start attempt.
#[derive(Debug)]
pub(crate) enum Started {
    Running(Console),
    /// The process exited before printing a startup line; the run holds
    /// its status and structured error for the S3-B1/S3-B3 failure steps.
    Exited(Run),
}

impl Started {
    pub(crate) fn running(&self) -> &Console {
        match self {
            Started::Running(console) => console,
            Started::Exited(run) => {
                panic!("the console exited instead of serving\n{}", run.describe())
            }
        }
    }

    pub(crate) fn exited(&self) -> &Run {
        match self {
            Started::Exited(run) => run,
            Started::Running(console) => panic!(
                "the console is serving at {}; expected a startup failure",
                console.startup
            ),
        }
    }
}

/// Spawns `card serve` with the caller's environment and arguments, then
/// waits for either one stdout line (the startup line) or process exit.
pub(crate) fn start_console(configure: impl FnOnce(&mut Command)) -> Started {
    let mut command = Command::new(CARD_BIN);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure(&mut command);
    let mut child = command.spawn().expect("failed to spawn the card binary");
    let stdout = child.stdout.take().expect("piped stdout");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let outcome = match reader.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line),
        };
        // The scenario may have given up already; a closed channel is
        // not an error for the reader.
        let _sent = sender.send((outcome, reader));
    });
    match receiver.recv_timeout(STARTUP_TIMEOUT) {
        Ok((Some(line), reader)) => {
            // Keep draining stdout so the server never blocks on a full
            // pipe while a scenario runs.
            std::thread::spawn(move || {
                let mut sink = reader;
                let mut rest = Vec::new();
                let _drained = sink.read_to_end(&mut rest);
            });
            let startup: Value = serde_json::from_str(line.trim())
                .unwrap_or_else(|error| panic!("startup line is not JSON ({error}): {line:?}"));
            let url = startup["url"]
                .as_str()
                .unwrap_or_else(|| panic!("startup line carries no url: {startup}"));
            let (host, port) = split_authority(url);
            Started::Running(Console {
                child,
                startup,
                host,
                port,
            })
        }
        Ok((None, _)) => Started::Exited(collect_exit(child, Vec::new())),
        Err(_) => {
            let _killed = child.kill();
            let output = child.wait_with_output().expect("wait for the console");
            panic!(
                "no startup line within {STARTUP_TIMEOUT:?}\nstderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

fn collect_exit(child: Child, stdout: Vec<u8>) -> Run {
    let mut output = child.wait_with_output().expect("wait for the console");
    output.stdout = stdout;
    let stderr_json = serde_json::from_slice(&output.stderr).ok();
    Run {
        output,
        stdout_json: None,
        stderr_json,
    }
}

/// Splits `http://host:port/...` into its host spelling (brackets kept
/// for IPv6) and port.
pub(crate) fn split_authority(url: &str) -> (String, u16) {
    let without_scheme = url
        .strip_prefix("http://")
        .unwrap_or_else(|| panic!("console url must be plain http: {url}"));
    let authority = without_scheme
        .split('/')
        .next()
        .expect("split always yields one item");
    let (host, port) = authority
        .rsplit_once(':')
        .unwrap_or_else(|| panic!("console url carries no port: {url}"));
    let port = port
        .parse()
        .unwrap_or_else(|error| panic!("console port is not a number ({error}): {url}"));
    (host.to_owned(), port)
}

/// Resolves a loopback host spelling to an address to connect to.
pub(crate) fn loopback_ip(host: &str) -> IpAddr {
    if host.eq_ignore_ascii_case("localhost") {
        return IpAddr::V4(Ipv4Addr::LOCALHOST);
    }
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    bare.parse()
        .unwrap_or_else(|error| panic!("unrecognised host {host:?}: {error}"))
}

/// Finds an address of this machine that is not loopback, for the S3-B2
/// refused-connection proof. The outbound route is asked first (no
/// packet is sent); with no route configured the interface table is
/// parsed instead, so the proof does not depend on network access.
pub(crate) fn non_loopback_local_ip() -> IpAddr {
    if let Some(ip) = UdpSocket::bind("0.0.0.0:0")
        .ok()
        .and_then(|socket| socket.connect("192.0.2.1:9").ok().map(|()| socket))
        .and_then(|socket| socket.local_addr().ok())
        .map(|addr| addr.ip())
        .filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
    {
        return ip;
    }
    let listing = Command::new("ifconfig")
        .arg("-a")
        .output()
        .or_else(|_| Command::new("ip").args(["-4", "addr"]).output())
        .expect("neither ifconfig nor ip is available to list interfaces");
    let text = String::from_utf8_lossy(&listing.stdout);
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let is_inet = words.next().is_some_and(|word| word == "inet");
            let address = words.next()?;
            is_inet
                .then(|| address.split('/').next())
                .flatten()
                .and_then(|a| a.parse::<Ipv4Addr>().ok())
        })
        .map(IpAddr::V4)
        .find(|ip| !ip.is_loopback() && !ip.is_unspecified())
        .expect("no non-loopback interface address found on this machine")
}

/// A raw request: method, target, and the exact Host field value to send.
#[derive(Debug, Clone)]
pub(crate) struct Request {
    pub(crate) method: String,
    pub(crate) target: String,
    pub(crate) host: String,
}

impl Console {
    pub(crate) fn connect_addr(&self) -> SocketAddr {
        SocketAddr::new(loopback_ip(&self.host), self.port)
    }

    /// The authority the server reported, used as the default Host.
    pub(crate) fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub(crate) fn get(&self, target: &str) -> Response {
        self.send(&Request {
            method: "GET".to_owned(),
            target: target.to_owned(),
            host: self.authority(),
        })
    }

    pub(crate) fn send(&self, request: &Request) -> Response {
        let addr = self.connect_addr();
        let mut stream = TcpStream::connect_timeout(&addr, SOCKET_TIMEOUT)
            .unwrap_or_else(|error| panic!("connect to {addr}: {error}"));
        stream
            .set_read_timeout(Some(SOCKET_TIMEOUT))
            .expect("read timeout");
        let wire = format!(
            "{} {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nAccept: text/html\r\n\r\n",
            request.method, request.target, request.host
        );
        stream
            .write_all(wire.as_bytes())
            .expect("write the request");
        let mut raw = Vec::new();
        // The server closes after one response; a read error after some
        // bytes means a reset mid-close, which the parser below reports.
        let _read = stream.read_to_end(&mut raw);
        parse_response(&raw, &request.target)
    }
}

fn parse_response(raw: &[u8], target: &str) -> Response {
    let head_end = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap_or_else(|| {
            panic!(
                "no header terminator in response to {target}: {:?}",
                String::from_utf8_lossy(raw)
            )
        });
    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
    let mut lines = head.lines();
    let status_line = lines.next().expect("status line");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("malformed status line for {target}: {status_line:?}"));
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect();
    let body_raw = &raw[head_end + 4..];
    let chunked = headers.iter().any(|(key, value)| {
        key.eq_ignore_ascii_case("transfer-encoding") && value.contains("chunked")
    });
    let body_bytes = if chunked {
        dechunk(body_raw)
    } else {
        body_raw.to_vec()
    };
    Response {
        status,
        headers,
        body: String::from_utf8_lossy(&body_bytes).into_owned(),
        target: target.to_owned(),
    }
}

fn dechunk(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut cursor = raw;
    while let Some(line_end) = cursor.windows(2).position(|w| w == b"\r\n") {
        let size_text = String::from_utf8_lossy(&cursor[..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("0").trim(), 16)
            .unwrap_or(0);
        if size == 0 {
            break;
        }
        let start = line_end + 2;
        let end = (start + size).min(cursor.len());
        out.extend_from_slice(&cursor[start..end]);
        cursor = cursor.get(end + 2..).unwrap_or(&[]);
    }
    out
}
