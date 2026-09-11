//! Regression tests for the CLI's machine-readable output contract.
#![cfg(unix)]

use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};

use serde_json::Value;

const CARD_BIN: &str = env!("CARGO_BIN_EXE_card");

#[test]
fn closed_stdout_reports_structured_output_failure() {
    let (reader, writer) = UnixStream::pair().expect("socket pair");
    drop(reader);
    let writer: OwnedFd = writer.into();
    let output = Command::new(CARD_BIN)
        .args(["--db", "unused-for-db-path.sqlite", "db-path"])
        .stdout(Stdio::from(writer))
        .stderr(Stdio::piped())
        .output()
        .expect("run card");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("stderr UTF-8");
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 1, "stderr: {stderr}");
    let error: Value =
        serde_json::from_str(lines.first().expect("stderr line")).expect("JSON stderr");
    let error_object = error.get("error").expect("error object");
    assert_eq!(
        error_object.get("category"),
        Some(&Value::String("storage".to_owned()))
    );
    let message = error_object
        .get("message")
        .expect("error message")
        .as_str()
        .expect("output failure message");
    assert!(!message.is_empty());
    assert!(message.contains("write command output"));
}

#[test]
fn db_path_success_is_one_stdout_json_line() {
    let output = Command::new(CARD_BIN)
        .args(["--db", "unused-for-db-path.sqlite", "db-path"])
        .output()
        .expect("run card");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout UTF-8");
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines.len(), 1);
    let value: Value =
        serde_json::from_str(lines.first().expect("stdout line")).expect("stdout JSON");
    assert_eq!(
        value.get("db_path"),
        Some(&Value::String("unused-for-db-path.sqlite".to_owned()))
    );
    assert_eq!(value.get("source"), Some(&Value::String("flag".to_owned())));
    assert!(output.stderr.is_empty());
}
