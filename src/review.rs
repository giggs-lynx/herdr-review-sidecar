//! Review mode: a flat list of the Markdown files in one directory. Review items (`F01.md`,
//! `Q01.md`, …) carry a `title` and a `status` in their frontmatter; any other file is titled by
//! its first heading and has no status.

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
    /// `None` for a file without frontmatter: it is listed with no status glyph.
    pub status: Option<Status>,
}

/// Whether a file is listed: any Markdown file that is not hidden.
pub fn is_item(name: &str) -> bool {
    !name.starts_with('.') && name.ends_with(".md")
}

/// The item id shown in the list: the file name without `.md` (`F01`).
pub fn item_id(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Split a leading `---` … `---` frontmatter block off `text`, reading its `title:` and
/// `status:` lines. Text without a closed block is all body and has no status; a missing title
/// falls back to the body's first `# ` heading.
pub fn parse(text: &str) -> (Meta, &str) {
    let (mut meta, body) = parse_frontmatter(text);
    if meta.title.is_empty() {
        meta.title = first_heading(body).to_string();
    }
    (meta, body)
}

fn first_heading(body: &str) -> &str {
    body.lines()
        .find_map(|line| line.strip_prefix("# "))
        .map(str::trim)
        .unwrap_or("")
}

fn parse_frontmatter(text: &str) -> (Meta, &str) {
    let Some(rest) = strip_line(text, "---") else {
        return (Meta::default(), text);
    };
    let mut meta = Meta {
        status: Some(Status::Open),
        ..Meta::default()
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
            meta.status = Some(Status::parse(unquote(v.trim())));
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

/// The first issue key (`ABC-123`) in `text`, matching what `grep -oE '[A-Z]+-[0-9]+' | head -1`
/// finds: the leftmost match, its uppercase run as long as possible.
pub fn issue_key(text: &str) -> Option<&str> {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if !b[i].is_ascii_uppercase() {
            i += 1;
            continue;
        }
        let letters_end = i + b[i..].iter().take_while(|c| c.is_ascii_uppercase()).count();
        let digits = b[letters_end..]
            .iter()
            .skip(1)
            .take_while(|c| c.is_ascii_digit())
            .count();
        if b.get(letters_end) == Some(&b'-') && digits > 0 {
            return Some(&text[i..letters_end + 1 + digits]);
        }
        i = letters_end;
    }
    None
}

/// The git checkout a review is about: the main repository's directory name (shared by its linked
/// worktrees) and the current branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    pub repo: String,
    pub branch: String,
}

/// Read the [`Checkout`] at `cwd` with read-only git queries.
pub fn checkout_of(cwd: &Path) -> Result<Checkout, String> {
    let git = |args: &[&str]| {
        crate::git::git_command(cwd, args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let common =
        git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]).ok_or_else(|| {
            format!(
                "no issue key in the workspace name, and not a git repo: {}",
                cwd.display()
            )
        })?;
    let branch = git(&["branch", "--show-current"])
        .filter(|b| !b.is_empty())
        .ok_or_else(|| format!("detached HEAD: {}", cwd.display()))?;
    let repo = Path::new(&common)
        .parent()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(Checkout { repo, branch })
}

/// The review directory under `base`, by the first rule that applies:
///
/// 1. an issue key in the herdr workspace name → `base/<key>`. It wins because an agent's cwd
///    often sits in a main checkout on some other branch, while the workspace is named for the work;
/// 2. an issue key in the branch name → `base/<key>`;
/// 3. otherwise `base/<repo>/<branch>`.
///
/// `checkout` is consulted only when the workspace name has no key.
pub fn review_dir_for(
    base: &Path,
    workspace_label: Option<&str>,
    checkout: impl FnOnce() -> Result<Checkout, String>,
) -> Result<PathBuf, String> {
    if let Some(key) = workspace_label.and_then(issue_key) {
        return Ok(base.join(key));
    }
    let checkout = checkout()?;
    Ok(match issue_key(&checkout.branch) {
        Some(key) => base.join(key),
        None => base.join(&checkout.repo).join(&checkout.branch),
    })
}

/// The name herdr shows for workspace `id`, or `None` when herdr cannot say.
pub fn workspace_label(herdr: &dyn crate::herdr::HerdrCli, id: &str) -> Option<String> {
    let json = herdr.run_json(&["workspace", "get", id]).ok()?;
    let value: serde_json::Value = serde_json::from_str(&json).ok()?;
    value["result"]["workspace"]["label"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// The review directory for the pane the launcher was invoked from (the focused pane's cwd, from
/// the host context) or, outside a launcher, for the current directory. Directories live under
/// `~/.cache/sidecar`; whoever writes review items asks the same binary, so writer and sidecar
/// always agree on the directory.
pub fn root_from_env() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .ok_or("HOME is not set")?;
    let base = PathBuf::from(home).join(".cache").join("sidecar");
    let json = std::env::var("HERDR_PLUGIN_CONTEXT_JSON").ok();
    let ctx =
        crate::host::parse_context(json.as_deref(), std::env::current_dir().unwrap_or_default());
    let workspace = std::env::var("HERDR_WORKSPACE_ID")
        .ok()
        .filter(|s| !s.is_empty())
        .or(ctx.workspace_id);
    let label = workspace.and_then(|id| workspace_label(&crate::herdr::LiveHerdr::from_env(), &id));
    review_dir_for(&base, label.as_deref(), || checkout_of(&ctx.cwd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_item_takes_every_markdown_file_that_is_not_hidden() {
        for yes in ["F01.md", "Q1.md", "notes.md", "README.md", "todo list.md"] {
            assert!(is_item(yes), "{yes}");
        }
        for no in [".hidden.md", "F01.txt", "F01.md.bak", ".socket"] {
            assert!(!is_item(no), "{no}");
        }
    }

    #[test]
    fn parse_reads_title_and_status_and_returns_the_body() {
        let (meta, body) = parse("---\ntitle: Pick a layer\nstatus: done\n---\nBody\n");
        assert_eq!(meta.title, "Pick a layer");
        assert_eq!(meta.status, Some(Status::Done));
        assert_eq!(body, "Body\n");
    }

    #[test]
    fn parse_handles_crlf_quotes_and_unknown_status() {
        let (meta, body) = parse("---\r\ntitle: \"a: b\"\r\nstatus: wip\r\n---\r\nx");
        assert_eq!(meta.title, "a: b");
        assert_eq!(meta.status, Some(Status::Open));
        assert_eq!(body, "x");
    }

    #[test]
    fn parse_treats_text_without_a_closed_block_as_body() {
        let text = "---\ntitle: never closed\n";
        assert_eq!(parse(text), (Meta::default(), text));
        assert_eq!(parse("intro\n"), (Meta::default(), "intro\n"));
    }

    #[test]
    fn a_file_without_a_title_takes_its_first_heading_and_one_without_frontmatter_has_no_status() {
        let (meta, body) = parse("intro\n# Things to do\n- one\n");
        assert_eq!(meta.title, "Things to do");
        assert_eq!(meta.status, None);
        assert_eq!(body, "intro\n# Things to do\n- one\n");
        let (meta, _) = parse("---\nstatus: done\n---\n# From the body\n");
        assert_eq!(meta.title, "From the body");
        assert_eq!(meta.status, Some(Status::Done));
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
    fn issue_key_finds_the_leftmost_key_like_grep() {
        assert_eq!(issue_key("ABC-123 fix"), Some("ABC-123"));
        assert_eq!(issue_key("feature/ABC-12-x"), Some("ABC-12"));
        assert_eq!(issue_key("xABC-1"), Some("ABC-1"));
        assert_eq!(issue_key("AB-x CD-9"), Some("CD-9"));
        assert_eq!(issue_key("review-sidecar"), None);
        assert_eq!(issue_key("ABC-"), None);
        assert_eq!(issue_key("abc-12"), None);
    }

    fn base() -> PathBuf {
        PathBuf::from("/c/sidecar")
    }

    fn checkout(branch: &str) -> Result<Checkout, String> {
        Ok(Checkout {
            repo: "app".into(),
            branch: branch.into(),
        })
    }

    #[test]
    fn a_key_in_the_workspace_name_wins_without_reading_git() {
        let dir = review_dir_for(&base(), Some("ABC-7 settings"), || {
            panic!("git is not consulted")
        });
        assert_eq!(dir, Ok(base().join("ABC-7")));
    }

    #[test]
    fn otherwise_the_branch_key_then_repo_and_branch() {
        assert_eq!(
            review_dir_for(&base(), Some("notes"), || checkout("feature/ABC-9-x")),
            Ok(base().join("ABC-9"))
        );
        assert_eq!(
            review_dir_for(&base(), None, || checkout("feat/review-mode")),
            Ok(base().join("app").join("feat/review-mode"))
        );
        assert_eq!(
            review_dir_for(&base(), None, || Err("not a git repo".into())),
            Err("not a git repo".to_string())
        );
    }

    struct Herdr(Option<&'static str>);
    impl crate::herdr::HerdrCli for Herdr {
        fn run_json(&self, args: &[&str]) -> std::io::Result<String> {
            assert_eq!(args, ["workspace", "get", "w7"]);
            self.0
                .map(str::to_owned)
                .ok_or_else(|| std::io::Error::other("no herdr"))
        }
    }

    #[test]
    fn workspace_label_reads_herdrs_workspace_name() {
        let json = r#"{"result":{"workspace":{"label":"ABC-7 settings"}}}"#;
        assert_eq!(
            workspace_label(&Herdr(Some(json)), "w7").as_deref(),
            Some("ABC-7 settings")
        );
        assert_eq!(workspace_label(&Herdr(None), "w7"), None);
        assert_eq!(workspace_label(&Herdr(Some("{}")), "w7"), None);
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
