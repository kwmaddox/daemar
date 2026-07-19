use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMPORARY_REPOSITORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[test]
fn invalid_request_reports_every_problem_without_creating_a_workflow_run() {
    let repository = TemporaryRepository::new();
    fs::create_dir(repository.path().join(".git")).expect(".git should be created");
    fs::write(
        repository.path().join(".git/HEAD"),
        b"ref: refs/heads/main\n",
    )
    .expect("HEAD fixture should be written");
    fs::write(repository.path().join("tracked.txt"), b"unchanged\n")
        .expect("tracked fixture should be written");
    fs::write(
        repository.path().join("invalid.json"),
        br#"{
            "schema": "change_request.v1",
            "id": "Not--Kebab-Case",
            "objective": "   ",
            "priority": "high",
            "acceptance_criteria": ["It works."]
        }"#,
    )
    .expect("request fixture should be written");
    let before = snapshot(repository.path());

    let output = Command::new(env!("CARGO_BIN_EXE_daemar"))
        .current_dir(repository.path())
        .args(["run", "invalid.json"])
        .output()
        .expect("Daemar should execute");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("diagnostics should be UTF-8"),
        concat!(
            "error: invalid Change Request - 3 problem(s) in invalid.json\n\n",
            "  [unknown_field] unknown field `priority`; change_request.v1 accepts exactly: ",
            "schema, id, objective, acceptance_criteria (at /priority)\n",
            "  [bad_slug] `id` must be lowercase kebab-case (a-z, 0-9, single dashes) ",
            "(at /id)\n",
            "  [blank_field] `objective` must not be blank (at /objective)\n\n",
            "no Workflow Run created\n",
        )
    );
    assert_eq!(snapshot(repository.path()), before);
    assert!(!repository.path().join(".daemar").exists());
}

#[test]
fn run_usage_errors_exit_two_before_preflight() {
    let output = Command::new(env!("CARGO_BIN_EXE_daemar"))
        .arg("run")
        .output()
        .expect("Daemar should execute");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .expect("usage error should be UTF-8")
            .contains("Usage: daemar run <CHANGE_REQUEST_PATH>")
    );
}

struct TemporaryRepository {
    path: PathBuf,
}

impl TemporaryRepository {
    fn new() -> Self {
        let sequence = TEMPORARY_REPOSITORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("daemar-per-22-{}-{sequence}", std::process::id()));
        fs::create_dir(&path).expect("temporary repository should be created");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryRepository {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).expect("temporary repository should be removed");
    }
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, current: &Path, entries: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        let mut children = fs::read_dir(current)
            .expect("snapshot directory should be readable")
            .collect::<Result<Vec<_>, _>>()
            .expect("snapshot entries should be readable");
        children.sort_by_key(fs::DirEntry::file_name);

        for child in children {
            let path = child.path();
            let relative = path
                .strip_prefix(root)
                .expect("snapshot path should be inside root")
                .to_owned();
            if path.is_dir() {
                entries.insert(relative, None);
                visit(root, &path, entries);
            } else {
                entries.insert(
                    relative,
                    Some(fs::read(path).expect("snapshot file should be readable")),
                );
            }
        }
    }

    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries);
    entries
}
