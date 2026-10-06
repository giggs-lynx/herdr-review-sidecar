//! the herdr plugin manifest is the Host Adapter's static surface.
//!
//! AC-17: the viewer declares a split-pane launch of the release binary.
//! This fork's manifest declares only the review sidecar (pane, action and launcher).
//! AC-N4: the viewer never auto-launches — the manifest declares no event hooks.
//!
//! Per the plan, these read `herdr-plugin.toml` to a string and assert on its contents.

use std::fs;
use std::path::PathBuf;

fn manifest_raw() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("herdr-plugin.toml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// The manifest with `#` line-comments stripped, so assertions match actual
/// declarations (table headers, keys/values) rather than prose in comments.
/// (The manifest uses no `#` inside string values, so cutting at the first `#`
/// per line is sufficient and keeps the test free of a TOML-parser dependency.)
fn manifest() -> String {
    manifest_raw()
        .lines()
        .map(|line| match line.find('#') {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn declares_no_windows_pane_entry() {
    // On Windows herdr cannot spawn the manifest's relative pane command (CreateProcessW resolves a
    // relative program against herdr's own dir, not any `--cwd`), so there is deliberately NO Windows
    // [[panes]] entry — the Windows launchers spawn the viewer by absolute path via
    // `pane split`/`tab create` + `pane run` (verified on real hardware, GH #58). Guard that we
    // didn't leave a dead `.exe` pane entry behind.
    let m = manifest();
    assert!(
        !m.contains(r#"id = "file-viewer-windows""#),
        "there must be no Windows pane entry (absolute-spawn launcher, not a manifest pane): {m}"
    );
    assert!(
        !m.contains("herdr-file-viewer.exe\"]"),
        "no [[panes]] command should name the .exe (the launcher spawns it by absolute path): {m}"
    );
}

#[test]
fn declares_at_least_one_action() {
    assert!(
        manifest().contains("[[actions]]"),
        "manifest must declare an [[actions]] entry to summon the viewer"
    );
}

#[test]
fn pins_minimum_herdr_version() {
    assert!(
        // 0.7.5 is the first herdr with `placement = "popup"` plugin panes (the root picker).
        manifest().contains(r#"min_herdr_version = "0.7.5""#),
        "manifest must pin min_herdr_version = \"0.7.5\""
    );
}

#[test]
fn declares_a_release_build_command() {
    let m = manifest();
    assert!(
        m.contains("[[build]]"),
        "manifest must declare a [[build]] step"
    );
    assert!(
        m.contains("scripts/fetch-or-build.sh"),
        "the build step must run the fetch-or-build script (prebuilt binary, cargo fallback)"
    );
}

#[test]
fn action_ids_are_unique() {
    // herdr rejects a manifest with two [[actions]] sharing an id (`duplicate_plugin_action_id`)
    // at LOAD time — before any platform filtering — so a same-id-per-platform pair would break
    // `herdr plugin install` on EVERY platform, not just Windows. Guard that every declared
    // action id is unique. (Verified live against herdr 0.7.1.)
    let m = manifest();
    let mut ids: Vec<&str> = m
        .lines()
        .filter_map(|l| l.trim().strip_prefix("id = \""))
        .filter_map(|s| s.strip_suffix('"'))
        // the [[panes]] entry also has an `id`; only action/pane ids share the key space we care
        // about here — dedupe checks all declared `id = "..."` values, which must each be unique.
        .collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        n,
        ids.len(),
        "every declared id in the manifest must be unique (herdr rejects duplicates at load)"
    );
}

#[test]
fn no_entry_declares_an_aarch64_windows_target() {
    // AC-N4: v1 targets x86_64-pc-windows-msvc only — no Windows-on-ARM declaration (comments
    // are stripped first, so this checks actual entries, not explanatory prose).
    assert!(
        !manifest().contains("aarch64"),
        "manifest must not declare any aarch64 (Windows-on-ARM) target"
    );
}

#[test]
fn declares_no_event_hooks() {
    let m = manifest();
    // AC-N4 (finder): no event-hook table — the viewer only ever opens via an explicit action.
    // AC-N6 (in-file-nav): search and go-to-line also have no auto/event trigger — they open
    // only via the explicit `/` (OpenSearch) and `:` (OpenGoToLine) key bindings. The manifest
    // declaring no `[[events]]` is the Host Adapter proof of this: nothing in the manifest
    // can cause herdr to call back into the viewer to open a prompt automatically.
    assert!(
        !m.contains("[[events]]"),
        "AC-N4/AC-N6: manifest must declare no [[events]] hooks"
    );
    assert!(
        !m.contains("[[link_handlers]]"),
        "manifest must declare no [[link_handlers]] (no automatic invocation path)"
    );
}

#[test]
fn declares_split_pane_launching_the_release_binary_in_review_mode() {
    let m = manifest();
    assert!(
        m.contains("[[panes]]"),
        "manifest must declare a [[panes]] entry"
    );
    assert!(
        m.contains(r#"placement = "split""#),
        "AC-17: the pane must declare placement = \"split\""
    );
    assert!(
        m.contains(r#"command = ["./target/release/herdr-file-viewer", "--review"]"#),
        "AC-17: the pane command must launch the release binary in review mode"
    );
}

#[test]
fn declares_linux_and_macos_platforms() {
    assert!(
        manifest().contains(r#"platforms = ["linux", "macos"]"#),
        "the review sidecar's shell launcher limits it to linux and macos"
    );
}

#[test]
fn declares_the_review_action() {
    let m = manifest();
    assert!(m.contains(r#"id = "open-review-sidecar""#));
    assert!(m.contains(r#"command = ["bash", "scripts/open-review-sidecar.sh"]"#));
}

#[test]
fn coexists_with_an_installed_herdr_file_viewer() {
    // Both plugins can be installed together only when their ids differ, and each launcher
    // toggles only its own pane only when the pane titles differ (launch decisions match by title).
    let m = manifest();
    assert!(m.contains(r#"id = "giggs.review-sidecar""#));
    assert!(!m.contains(r#"id = "herdr-file-viewer""#));
    assert!(m.contains(&format!(
        "title = \"{}\"",
        herdr_file_viewer::launch::REVIEW_LABEL
    )));
    assert!(!m.contains(r#"title = "Files""#));

    let launcher = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/open-review-sidecar.sh"),
    )
    .unwrap();
    assert!(launcher.contains(r#"plugin_id="giggs.review-sidecar""#));
    assert!(launcher.contains("--entrypoint review"));
    assert!(launcher.contains("--launch-decision-review"));
}
