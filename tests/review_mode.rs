//! Review mode (`--review`): the tree lists only review items, labelled and dimmed from their
//! frontmatter.

mod common;

use common::{NoopContent, NoopEditor, NoopGit, RecordingClipboard, TempDir};
use herdr_file_viewer::controller::{
    Components, Controller, REVIEW_RESCAN_INTERVAL, RootProviders,
};
use herdr_file_viewer::git::Baseline;
use herdr_file_viewer::presenter::draw;
use herdr_file_viewer::review::Status;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn review_controller(dir: &TempDir) -> Controller {
    let components = Components {
        providers: Box::new(|_| RootProviders {
            git: Arc::new(NoopGit),
            content: Box::new(NoopContent),
        }),
        editor: Box::new(NoopEditor),
        clipboard: Box::new(RecordingClipboard::default()),
        renderers: None,
    };
    let mut ctrl = Controller::new(
        common::resolved(dir.path().to_path_buf(), false),
        Baseline::Head,
        components,
    );
    ctrl.apply_review_mode();
    ctrl
}

fn seed(dir: &TempDir) {
    let write = |name: &str, text: &str| fs::write(dir.path().join(name), text).unwrap();
    write("F01.md", "---\ntitle: Null check\nstatus: open\n---\nbody");
    write("Q01.md", "---\ntitle: Which layer\nstatus: done\n---\nbody");
    write(
        "Q02.md",
        "---\ntitle: Dropped idea\nstatus: dropped\n---\nbody",
    );
    write("notes.md", "not an item");
    write(".socket", "x");
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/F09.md"), "nested").unwrap();
}

#[test]
fn lists_only_root_items_with_their_frontmatter() {
    let dir = TempDir::new();
    seed(&dir);
    let ctrl = review_controller(&dir);

    let state = ctrl.view_state();
    let rows: Vec<_> = state
        .nodes
        .iter()
        .map(|n| {
            let meta = n.review.as_ref().expect("every row carries frontmatter");
            (
                n.path.file_name().unwrap().to_string_lossy().into_owned(),
                meta.title.clone(),
                meta.status,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("F01.md".into(), "Null check".into(), Status::Open),
            ("Q01.md".into(), "Which layer".into(), Status::Done),
            ("Q02.md".into(), "Dropped idea".into(), Status::Dropped),
        ]
    );
}

#[test]
fn draws_status_glyph_id_and_title_and_dims_closed_items() {
    let dir = TempDir::new();
    seed(&dir);
    let ctrl = review_controller(&dir);

    let mut terminal = Terminal::new(TestBackend::new(100, 10)).unwrap();
    let state = ctrl.view_state();
    terminal
        .draw(|f| {
            draw(f, &state);
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let row_of = |needle: &str| {
        (0..buffer.area.height)
            .find(|&y| {
                let line: String = (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect();
                line.contains(needle)
            })
            .unwrap_or_else(|| panic!("no row shows {needle:?}"))
    };
    let modifier_at = |needle: &str| {
        let y = row_of(needle);
        let x = (0..buffer.area.width)
            .find(|&x| buffer[(x, y)].symbol() == needle.chars().next().unwrap().to_string())
            .unwrap();
        buffer[(x, y)].modifier
    };

    let open = modifier_at("○ F01  Null check");
    assert!(
        open.contains(Modifier::REVERSED),
        "the first item is selected"
    );
    assert!(!open.contains(Modifier::DIM));
    assert!(modifier_at("✓ Q01  Which layer").contains(Modifier::DIM));
    assert!(modifier_at("✗ Q02  Dropped idea").contains(Modifier::DIM));
}

const LOADING_PLACEHOLDER: &str = "Rendering\u{2026}";

fn flatten(ctrl: &Controller) -> String {
    ctrl.content()
        .lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn settled(dir: &TempDir) -> Controller {
    let mut ctrl = review_controller(dir);
    let limit = Instant::now() + Duration::from_secs(10);
    while flatten(&ctrl) == LOADING_PLACEHOLDER {
        ctrl.poll();
        assert!(Instant::now() < limit, "the first render never landed");
        std::thread::sleep(Duration::from_millis(5));
    }
    ctrl
}

fn selected_name(ctrl: &Controller) -> String {
    let state = ctrl.view_state();
    state.nodes[state.selected]
        .path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[test]
fn an_unchanged_directory_dispatches_nothing() {
    let dir = TempDir::new();
    seed(&dir);
    let mut ctrl = settled(&dir);
    let seq = ctrl.render_seq();

    assert!(!ctrl.tick_review_watch(Instant::now()));
    assert_eq!(ctrl.render_seq(), seq);
}

#[test]
fn editing_the_displayed_item_rerenders_it_in_place() {
    let dir = TempDir::new();
    seed(&dir);
    let mut ctrl = settled(&dir);
    let seq = ctrl.render_seq();

    fs::write(
        dir.path().join("F01.md"),
        "---\ntitle: Null check\nstatus: done\n---\nlonger body",
    )
    .unwrap();
    assert!(ctrl.tick_review_watch(Instant::now()));

    assert_eq!(ctrl.render_seq(), seq + 1);
    assert_ne!(
        flatten(&ctrl),
        LOADING_PLACEHOLDER,
        "an in-place re-render keeps the current body (and its scroll) until the new one lands"
    );
    assert_eq!(
        ctrl.view_state().nodes[0].review.as_ref().unwrap().status,
        Status::Done
    );
}

#[test]
fn rescans_at_most_once_per_interval() {
    let dir = TempDir::new();
    seed(&dir);
    let mut ctrl = settled(&dir);
    let t0 = Instant::now();
    assert!(!ctrl.tick_review_watch(t0));

    fs::write(dir.path().join("Q03.md"), "---\ntitle: New\n---\n").unwrap();
    assert!(!ctrl.tick_review_watch(t0 + REVIEW_RESCAN_INTERVAL / 2));
    assert!(ctrl.tick_review_watch(t0 + REVIEW_RESCAN_INTERVAL));
}

#[test]
fn a_new_item_is_listed_without_touching_the_displayed_one() {
    let dir = TempDir::new();
    seed(&dir);
    let mut ctrl = settled(&dir);
    let seq = ctrl.render_seq();

    fs::write(dir.path().join("Q03.md"), "---\ntitle: New\n---\n").unwrap();
    assert!(ctrl.tick_review_watch(Instant::now()));

    assert_eq!(ctrl.render_seq(), seq, "the displayed item did not change");
    assert_eq!(ctrl.view_state().nodes.len(), 4);
    assert_eq!(selected_name(&ctrl), "F01.md");
}

#[test]
fn removing_the_displayed_item_renders_the_neighbour() {
    let dir = TempDir::new();
    seed(&dir);
    let mut ctrl = settled(&dir);
    let seq = ctrl.render_seq();

    fs::remove_file(dir.path().join("F01.md")).unwrap();
    assert!(ctrl.tick_review_watch(Instant::now()));

    assert_eq!(ctrl.render_seq(), seq + 1);
    assert_eq!(selected_name(&ctrl), "Q01.md");
}

#[test]
fn the_first_item_in_an_empty_directory_is_rendered() {
    let dir = TempDir::new();
    let mut ctrl = settled(&dir);
    let seq = ctrl.render_seq();

    fs::write(dir.path().join("F01.md"), "---\ntitle: First\n---\n").unwrap();
    assert!(ctrl.tick_review_watch(Instant::now()));

    assert_eq!(ctrl.render_seq(), seq + 1);
    assert_eq!(selected_name(&ctrl), "F01.md");
}
