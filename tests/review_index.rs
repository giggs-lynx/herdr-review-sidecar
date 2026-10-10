//! Review mode's directory index: which item files it lists and when it reports a change.

mod common;

use common::TempDir;
use herdr_file_viewer::review::{ReviewIndex, Status};
use std::fs;

fn write(dir: &TempDir, name: &str, text: &str) {
    fs::write(dir.path().join(name), text).unwrap();
}

#[test]
fn indexes_every_markdown_file_with_its_frontmatter() {
    let dir = TempDir::new();
    write(&dir, "F01.md", "---\ntitle: One\nstatus: done\n---\nbody");
    write(&dir, "notes.md", "---\ntitle: Not an item\n---\n");
    write(&dir, ".socket", "x");
    fs::create_dir(dir.path().join("Q01.md")).unwrap();

    let index = ReviewIndex::new(dir.path().to_path_buf());
    let metas = index.metas();

    assert_eq!(metas.len(), 2);
    let meta = index.meta(&dir.path().join("F01.md")).unwrap();
    assert_eq!(meta.title, "One");
    assert_eq!(meta.status, Some(Status::Done));
    let notes = index.meta(&dir.path().join("notes.md")).unwrap();
    assert_eq!(notes.title, "Not an item");
    assert_eq!(notes.status, Some(Status::Open));
}

#[test]
fn rescan_reports_added_changed_and_removed_items_once() {
    let dir = TempDir::new();
    write(&dir, "Q01.md", "---\ntitle: a\n---\n");
    write(&dir, "Q02.md", "---\ntitle: b\n---\n");
    let mut index = ReviewIndex::new(dir.path().to_path_buf());
    assert!(index.rescan().is_empty());

    write(
        &dir,
        "Q01.md",
        "---\ntitle: a, longer\nstatus: dropped\n---\n",
    );
    fs::remove_file(dir.path().join("Q02.md")).unwrap();
    write(&dir, "Q03.md", "---\ntitle: c\n---\n");

    assert_eq!(
        index.rescan(),
        ["Q01.md", "Q02.md", "Q03.md"].map(|n| dir.path().join(n))
    );
    assert_eq!(
        index.meta(&dir.path().join("Q01.md")).unwrap().status,
        Some(Status::Dropped)
    );
    assert!(index.meta(&dir.path().join("Q02.md")).is_none());
    assert!(index.rescan().is_empty());
}
