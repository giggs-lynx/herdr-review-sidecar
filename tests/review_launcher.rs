//! `scripts/open-review-sidecar.sh`: asks `--review-root` for the directory, creates it, and opens
//! the sidecar's own pane with it; without a directory it notifies instead of opening.
#![cfg(unix)]

mod common;

use common::TempDir;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

fn executable(path: &Path) {
    let mut permissions = std::fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).unwrap();
}

struct Run {
    output: Output,
    calls: String,
}

fn run_launcher(temp: &TempDir, workspace_label: Option<&str>, panes_json: Option<&str>) -> Run {
    let plugin_root = temp.path().join("plugin");
    let scripts_dir = plugin_root.join("scripts");
    let release_dir = plugin_root.join("target/release");
    let home = temp.path().join("home");
    for dir in [&scripts_dir, &release_dir, &home] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let launcher = scripts_dir.join("open-review-sidecar.sh");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/open-review-sidecar.sh"),
        &launcher,
    )
    .unwrap();
    executable(&launcher);
    let viewer = release_dir.join("herdr-file-viewer");
    std::fs::copy(env!("CARGO_BIN_EXE_herdr-file-viewer"), &viewer).unwrap();
    executable(&viewer);

    let capture = temp.path().join("herdr-calls");
    let fake_herdr = temp.path().join("herdr-stub");
    std::fs::write(
        &fake_herdr,
        r#"#!/bin/sh
printf '%s\n' "$*" >> "$HERDR_CAPTURE"
case "$1 $2" in
  "pane list") if [ -n "${HERDR_PANES_JSON:-}" ]; then printf '%s\n' "$HERDR_PANES_JSON"; fi ;;
  "workspace get") [ -n "${HERDR_TEST_WS_LABEL:-}" ] && printf '{"result":{"workspace":{"label":"%s"}}}\n' "$HERDR_TEST_WS_LABEL" ;;
esac
exit 0
"#,
    )
    .unwrap();
    executable(&fake_herdr);

    let mut command = Command::new(&launcher);
    command
        .env("HERDR_BIN_PATH", &fake_herdr)
        .env("HERDR_CAPTURE", &capture)
        .env(
            "HERDR_PLUGIN_CONTEXT_JSON",
            format!(r#"{{"focused_pane_cwd":"{}"}}"#, temp.path().display()),
        )
        .env("HERDR_WORKSPACE_ID", "w7")
        .env("HOME", &home);
    match workspace_label {
        Some(label) => command.env("HERDR_TEST_WS_LABEL", label),
        None => command.env_remove("HERDR_TEST_WS_LABEL"),
    };
    match panes_json {
        Some(json) => command.env("HERDR_PANES_JSON", json),
        None => command.env_remove("HERDR_PANES_JSON"),
    };
    let output = command.output().unwrap();
    let calls = std::fs::read_to_string(&capture).unwrap_or_default();
    Run { output, calls }
}

#[test]
fn opens_the_review_pane_at_the_directory_for_the_workspace() {
    let temp = TempDir::new();
    let run = run_launcher(&temp, Some("ABC-7 settings"), None);

    assert!(run.output.status.success(), "{:?}", run.output);
    let target = temp.path().join("home/.cache/sidecar/ABC-7");
    assert!(target.is_dir(), "the launcher creates the directory");
    assert!(
        run.calls.lines().any(|l| l == "workspace get w7"),
        "the workspace name comes from herdr: {}",
        run.calls
    );
    let open = run
        .calls
        .lines()
        .find(|l| l.starts_with("plugin pane open"))
        .expect("a pane is opened");
    assert!(
        open.contains("--plugin giggs.review-sidecar --entrypoint review"),
        "{open}"
    );
    assert!(
        open.contains(&format!(
            "--env HERDR_FILE_VIEWER_ROOT={}",
            target.display()
        )),
        "{open}"
    );
    assert!(!open.contains("--cwd"), "{open}");
}

#[test]
fn without_a_directory_it_notifies_and_opens_nothing() {
    let temp = TempDir::new();
    let run = run_launcher(&temp, Some("notes"), None);

    assert!(!run.output.status.success());
    assert!(
        run.calls
            .lines()
            .any(|l| l.starts_with("notification show") && l.contains("not a git repo")),
        "{}",
        run.calls
    );
    assert!(!run.calls.contains("plugin pane open"), "{}", run.calls);
}

#[test]
fn a_focused_review_pane_is_closed_and_a_files_pane_is_left_alone() {
    let temp = TempDir::new();
    let panes = r#"{"result":{"panes":[
        {"pane_id":"p1","label":"Files","focused":false,"tab_id":"t1"},
        {"pane_id":"p2","label":"Review","focused":true,"tab_id":"t1"}]}}"#;
    let run = run_launcher(&temp, None, Some(panes));

    assert!(run.output.status.success());
    assert!(
        run.calls.lines().any(|l| l == "pane close p2"),
        "{}",
        run.calls
    );
    assert!(!run.calls.contains("p1"), "{}", run.calls);
}
