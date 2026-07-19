use std::process::Command;

#[test]
fn help_exposes_the_workflow_run_command_surface() {
    let output = Command::new(env!("CARGO_BIN_EXE_daemar"))
        .arg("--help")
        .output()
        .expect("Daemar should be executable");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("help should be UTF-8");
    let commands = command_names(&stdout);

    assert!(commands.contains(&"run"));
    assert!(commands.contains(&"runs"));
    assert!(commands.contains(&"show"));
}

#[test]
fn command_help_names_the_approved_arguments() {
    for (command, argument) in [
        ("run", "<CHANGE_REQUEST_PATH>"),
        ("runs", "[CHANGE_REQUEST_SLUG]"),
        ("show", "<WORKFLOW_RUN_ID_OR_PREFIX>"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_daemar"))
            .args([command, "--help"])
            .output()
            .expect("Daemar should be executable");

        assert!(output.status.success());

        let stdout = String::from_utf8(output.stdout).expect("help should be UTF-8");
        assert!(stdout.contains(argument), "{command} help:\n{stdout}");
    }
}

fn command_names(help: &str) -> Vec<&str> {
    help.lines()
        .skip_while(|line| line.trim() != "Commands:")
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .collect()
}
