//! `--review-root`: the review directory for a piece of work, derived from the herdr workspace
//! name and the git checkout.

mod common;

use common::{TempDir, git, init_repo_with_commit};
use herdr_file_viewer::review::{Checkout, checkout_of};
use std::path::Path;
use std::process::Command;

#[test]
fn checkout_names_the_main_repository_even_from_a_linked_worktree() {
    let temp = TempDir::new();
    let main = temp.path().join("app");
    std::fs::create_dir(&main).unwrap();
    init_repo_with_commit(&main);
    git(&main, &["checkout", "-q", "-b", "feature/ABC-12-units"]);
    let linked = temp.path().join("wt-ABC-12");
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "fix/other",
            linked.to_str().unwrap(),
        ],
    );

    assert_eq!(
        checkout_of(&main),
        Ok(Checkout {
            repo: "app".into(),
            branch: "feature/ABC-12-units".into()
        })
    );
    assert_eq!(
        checkout_of(&linked),
        Ok(Checkout {
            repo: "app".into(),
            branch: "fix/other".into()
        })
    );
}

#[test]
fn outside_a_repository_there_is_no_checkout() {
    let temp = TempDir::new();
    let err = checkout_of(temp.path()).unwrap_err();
    assert!(err.contains("not a git repo"), "{err}");
}

fn review_root(home: &Path, cwd: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_herdr-file-viewer"))
        .arg("--review-root")
        .current_dir(cwd)
        .env("HOME", home)
        .env_remove("HERDR_PLUGIN_CONTEXT_JSON")
        .env_remove("HERDR_WORKSPACE_ID")
        .output()
        .unwrap()
}

#[test]
fn the_binary_prints_the_directory_for_its_working_directory() {
    let temp = TempDir::new();
    let repo = temp.path().join("app");
    std::fs::create_dir(&repo).unwrap();
    init_repo_with_commit(&repo);
    git(&repo, &["checkout", "-q", "-b", "feat/review-mode"]);

    let out = review_root(temp.path(), &repo);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        temp.path()
            .join(".cache/sidecar/app/feat/review-mode")
            .display()
            .to_string()
    );
}

#[test]
fn the_binary_explains_why_it_has_no_directory() {
    let temp = TempDir::new();
    let out = review_root(temp.path(), temp.path());
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not a git repo"),
        "{out:?}"
    );
}
