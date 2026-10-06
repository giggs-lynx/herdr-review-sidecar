//! Review mode: a flat list of review items (`F01.md`, `Q01.md`, …) in one directory, each a
//! markdown file whose frontmatter carries a `title` and a `status`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// An item's `status:` frontmatter value; anything other than `done` / `dropped` reads as open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Open,
    Done,
    Dropped,
}

impl Status {
    fn parse(value: &str) -> Self {
        match value {
            "done" => Status::Done,
            "dropped" => Status::Dropped,
            _ => Status::Open,
        }
    }

    pub fn glyph(self) -> char {
        match self {
            Status::Open => '○',
            Status::Done => '✓',
            Status::Dropped => '✗',
        }
    }

    pub fn is_closed(self) -> bool {
        self != Status::Open
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Meta {
    pub title: String,
    pub status: Status,
}

/// Whether a file name is a review item, with the shell glob `[A-Z][0-9]*.md`'s semantics.
pub fn is_item(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 5 && b[0].is_ascii_uppercase() && b[1].is_ascii_digit() && name.ends_with(".md")
}

/// The item id shown in the list: the file name without `.md` (`F01`).
pub fn item_id(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Split a leading `---` … `---` frontmatter block off `text`, reading its `title:` and
/// `status:` lines. Text without a closed block is all body.
pub fn parse(text: &str) -> (Meta, &str) {
    let mut meta = Meta::default();
    let Some(rest) = strip_line(text, "---") else {
        return (meta, text);
    };
    let mut cursor = rest;
    while !cursor.is_empty() {
        let (line, next) = cursor.split_once('\n').unwrap_or((cursor, ""));
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line == "---" {
            return (meta, next);
        }
        if let Some(v) = line.strip_prefix("title:") {
            meta.title = unquote(v.trim()).to_string();
        } else if let Some(v) = line.strip_prefix("status:") {
            meta.status = Status::parse(unquote(v.trim()));
        }
        cursor = next;
    }
    (Meta::default(), text)
}

/// The markdown handed to the renderer: a `# <id> <title>` heading, then the body without its
/// frontmatter.
pub fn display_source(id: &str, text: &str) -> String {
    let (meta, body) = parse(text);
    let heading = if meta.title.is_empty() {
        format!("# {id}")
    } else {
        format!("# {id} {}", meta.title)
    };
    format!("{heading}\n\n{}", body.trim_start_matches(['\r', '\n']))
}

/// [`display_source`] applied to a classified file, leaving placeholders untouched.
pub fn prepare(path: &Path, prepared: crate::render::Prepared) -> crate::render::Prepared {
    use crate::render::Prepared;
    let id = item_id(path);
    match prepared {
        Prepared::Full { text } => Prepared::Full {
            text: display_source(&id, &text),
        },
        Prepared::Truncated { text, notice } => Prepared::Truncated {
            text: display_source(&id, &text),
            notice,
        },
        other => other,
    }
}

fn strip_line<'a>(text: &'a str, line: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(line)?;
    rest.strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
}

fn unquote(v: &str) -> &str {
    ['"', '\'']
        .iter()
        .find_map(|q| v.strip_prefix(*q).and_then(|v| v.strip_suffix(*q)))
        .unwrap_or(v)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

/// The review items in one directory, keyed by path, re-read only when a file's modification
/// time or size changes.
#[derive(Debug, Default)]
pub struct ReviewIndex {
    dir: PathBuf,
    items: BTreeMap<PathBuf, (Stamp, Meta)>,
}

impl ReviewIndex {
    pub fn new(dir: PathBuf) -> Self {
        let mut index = ReviewIndex {
            dir,
            items: BTreeMap::new(),
        };
        index.rescan();
        index
    }

    /// Stat the directory again and return every item path that was added, changed or removed.
    pub fn rescan(&mut self) -> Vec<PathBuf> {
        let mut seen = BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if !name.to_str().is_some_and(is_item) {
                    continue;
                }
                let Ok(md) = entry.metadata() else { continue };
                if md.is_file() {
                    let stamp = Stamp {
                        modified: md.modified().ok(),
                        len: md.len(),
                    };
                    seen.insert(entry.path(), stamp);
                }
            }
        }
        let mut changed: Vec<PathBuf> = self
            .items
            .keys()
            .filter(|p| !seen.contains_key(*p))
            .cloned()
            .collect();
        self.items.retain(|p, _| seen.contains_key(p));
        for (path, stamp) in seen {
            if self.items.get(&path).is_some_and(|(s, _)| *s == stamp) {
                continue;
            }
            let text = std::fs::read(&path)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            let meta = parse(&text).0;
            self.items.insert(path.clone(), (stamp, meta));
            changed.push(path);
        }
        changed.sort();
        changed
    }

    pub fn meta(&self, path: &Path) -> Option<&Meta> {
        self.items.get(path).map(|(_, m)| m)
    }

    pub fn metas(&self) -> BTreeMap<PathBuf, Meta> {
        self.items
            .iter()
            .map(|(p, (_, m))| (p.clone(), m.clone()))
            .collect()
    }
}

/// The review directory named by [`crate::host::ROOT_ENV`]: it must be an absolute, existing
/// directory. `is_dir` is injected so the check is testable without touching the filesystem.
pub fn review_root(raw: Option<&str>, is_dir: impl Fn(&Path) -> bool) -> Result<PathBuf, String> {
    let Some(raw) = raw.filter(|s| !s.trim().is_empty()) else {
        return Err(format!("{} is not set", crate::host::ROOT_ENV));
    };
    let dir = PathBuf::from(raw);
    if !dir.is_absolute() {
        return Err(format!(
            "{} is not an absolute path: {raw}",
            crate::host::ROOT_ENV
        ));
    }
    if !is_dir(&dir) {
        return Err(format!("not a directory: {raw}"));
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_item_follows_the_shell_glob() {
        for yes in ["F01.md", "Q1.md", "A9-notes.md", "Z00.md"] {
            assert!(is_item(yes), "{yes}");
        }
        for no in [
            "f01.md",
            "F.md",
            "FA1.md",
            "F01.txt",
            "F01.md.bak",
            "README.md",
            "1F.md",
        ] {
            assert!(!is_item(no), "{no}");
        }
    }

    #[test]
    fn parse_reads_title_and_status_and_returns_the_body() {
        let (meta, body) = parse("---\ntitle: Pick a layer\nstatus: done\n---\nBody\n");
        assert_eq!(meta.title, "Pick a layer");
        assert_eq!(meta.status, Status::Done);
        assert_eq!(body, "Body\n");
    }

    #[test]
    fn parse_handles_crlf_quotes_and_unknown_status() {
        let (meta, body) = parse("---\r\ntitle: \"a: b\"\r\nstatus: wip\r\n---\r\nx");
        assert_eq!(meta.title, "a: b");
        assert_eq!(meta.status, Status::Open);
        assert_eq!(body, "x");
    }

    #[test]
    fn parse_treats_text_without_a_closed_block_as_body() {
        let text = "---\ntitle: never closed\n";
        assert_eq!(parse(text), (Meta::default(), text));
        assert_eq!(parse("# plain\n"), (Meta::default(), "# plain\n"));
    }

    #[test]
    fn display_source_replaces_frontmatter_with_an_id_heading() {
        assert_eq!(
            display_source("Q01", "---\ntitle: Pick\nstatus: open\n---\n\nBody\n"),
            "# Q01 Pick\n\nBody\n"
        );
        assert_eq!(display_source("F02", "Body"), "# F02\n\nBody");
    }

    #[test]
    fn status_glyphs_and_closed_flag() {
        assert_eq!(Status::Open.glyph(), '○');
        assert_eq!(Status::Done.glyph(), '✓');
        assert_eq!(Status::Dropped.glyph(), '✗');
        assert!(!Status::Open.is_closed());
        assert!(Status::Done.is_closed() && Status::Dropped.is_closed());
    }

    #[test]
    fn review_root_accepts_an_absolute_existing_directory() {
        assert_eq!(
            review_root(Some("/reviews/set-a"), |_| true),
            Ok(PathBuf::from("/reviews/set-a"))
        );
    }

    #[test]
    fn review_root_rejects_unset_relative_and_missing_paths() {
        assert!(review_root(None, |_| true).is_err());
        assert!(review_root(Some("  "), |_| true).is_err());
        assert!(review_root(Some("reviews/set-a"), |_| true).is_err());
        assert!(review_root(Some("/reviews/set-a"), |_| false).is_err());
    }
}
