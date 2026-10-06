//! Review mode (`--review`): the tree lists only review items, labelled and dimmed from their
//! frontmatter.

mod common;

use common::{NoopContent, NoopEditor, NoopGit, RecordingClipboard, TempDir};
use herdr_file_viewer::controller::{Components, Controller, RootProviders};
use herdr_file_viewer::git::Baseline;
use herdr_file_viewer::presenter::draw;
use herdr_file_viewer::review::Status;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Modifier;
use std::fs;
use std::sync::Arc;

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
