use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn invalid_request_creates_no_workflow_run_and_does_not_mutate_the_repository() {
    let repository = temporary_git_repository("missing-fields");
    let request_path = repository.join("change-request.json");
    let request_bytes = br#"{"schema":"change_request.v1"}"#;
    fs::write(&request_path, request_bytes).expect("fixture should be writable");
    commit_all(&repository, "fixture");
    let head_before = git_output(&repository, &["rev-parse", "HEAD"]);
    let refs_before = git_output(&repository, &["show-ref", "--head"]);
    let status_before = git_output(
        &repository,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    );
    let worktrees_before = git_output(&repository, &["worktree", "list", "--porcelain"]);
    let config_before = fs::read(repository.join(".git/config")).expect("Git config should exist");
    let entries_before = directory_entries(&repository);

    let output = Command::new(env!("CARGO_BIN_EXE_daemar"))
        .args(["run", request_path.to_str().expect("UTF-8 fixture path")])
        .current_dir(&repository)
        .output()
        .expect("Daemar should be executable");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostics should be UTF-8");
    assert_eq!(
        stderr,
        format!(
            "error: invalid Change Request - 3 problem(s) in {}\n\n\
             \x20 [missing_field] missing required field `id` (at /id)\n\
             \x20 [missing_field] missing required field `objective` (at /objective)\n\
             \x20 [missing_field] missing required field `acceptance_criteria` (at /acceptance_criteria)\n\n\
             no Workflow Run created\n",
            request_path.display()
        )
    );
    assert_eq!(git_output(&repository, &["rev-parse", "HEAD"]), head_before);
    assert_eq!(
        git_output(&repository, &["show-ref", "--head"]),
        refs_before
    );
    assert_eq!(
        git_output(
            &repository,
            &["status", "--porcelain=v1", "--untracked-files=all"]
        ),
        status_before
    );
    assert_eq!(
        git_output(&repository, &["worktree", "list", "--porcelain"]),
        worktrees_before
    );
    assert_eq!(
        fs::read(repository.join(".git/config")).expect("Git config should remain readable"),
        config_before
    );
    assert_eq!(directory_entries(&repository), entries_before);
    assert!(!repository.join(".daemar").exists());

    fs::remove_dir_all(repository).expect("fixture should be removable");
}

#[test]
fn unreadable_request_is_an_invalid_request_not_a_usage_error() {
    let repository = temporary_git_repository("missing-file");
    let request_path = repository.join("does-not-exist.json");
    let output = Command::new(env!("CARGO_BIN_EXE_daemar"))
        .args(["run", request_path.to_str().expect("UTF-8 fixture path")])
        .current_dir(&repository)
        .output()
        .expect("Daemar should be executable");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostics should be UTF-8");
    assert!(stderr.contains("[io_error]"), "stderr:\n{stderr}");
    assert!(stderr.contains("(at /)"), "stderr:\n{stderr}");
    assert!(
        stderr.ends_with("no Workflow Run created\n"),
        "stderr:\n{stderr}"
    );
    assert!(!repository.join(".daemar").exists());

    fs::remove_dir_all(repository).expect("fixture should be removable");
}

#[test]
fn usage_errors_exit_two_without_preflight_diagnostics() {
    let output = Command::new(env!("CARGO_BIN_EXE_daemar"))
        .arg("run")
        .output()
        .expect("Daemar should be executable");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("usage diagnostics should be UTF-8");
    assert!(stderr.contains("Usage:"), "stderr:\n{stderr}");
    assert!(
        !stderr.contains("no Workflow Run created"),
        "stderr:\n{stderr}"
    );
}

fn temporary_git_repository(label: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should follow the Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("daemar-{label}-{}-{unique}", std::process::id()));
    fs::create_dir(&path).expect("fixture repository should be creatable");
    git(&path, &["init", "--quiet"]);
    git(&path, &["config", "user.name", "Daemar Test"]);
    git(&path, &["config", "user.email", "daemar@example.invalid"]);
    path
}

fn commit_all(repository: &std::path::Path, message: &str) {
    git(repository, &["add", "."]);
    git(repository, &["commit", "--quiet", "-m", message]);
}

fn git(repository: &std::path::Path, arguments: &[&str]) {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .expect("Git should execute in the fixture repository");
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_output(repository: &std::path::Path, arguments: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .expect("Git should execute in the fixture repository");
    assert!(output.status.success(), "git {arguments:?} should succeed");
    output.stdout
}

fn directory_entries(path: &std::path::Path) -> Vec<std::ffi::OsString> {
    let mut entries: Vec<_> = fs::read_dir(path)
        .expect("fixture repository should be readable")
        .map(|entry| entry.expect("fixture entry should be readable").file_name())
        .collect();
    entries.sort();
    entries
}
