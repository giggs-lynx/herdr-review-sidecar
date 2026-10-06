//! Review mode: a flat list of review items (`F01.md`, `Q01.md`, …) in one directory, each a
//! markdown file whose frontmatter carries a `title` and a `status`.

use std::path::{Path, PathBuf};

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
