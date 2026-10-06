//! Review mode's live directory: re-read the review items as files are added, edited or removed.

use super::*;

/// How often review mode stats its directory for added, changed or removed items.
pub const REVIEW_RESCAN_INTERVAL: Duration = Duration::from_secs(1);

impl Controller {
    /// Rescan the review directory at most once per [`REVIEW_RESCAN_INTERVAL`]; returns whether
    /// anything changed and the view needs a redraw.
    ///
    /// An edit to the displayed item re-renders it in place, keeping the scroll position. When the
    /// selection no longer matches what is displayed (the displayed item was removed, or the first
    /// item appeared in an empty directory), the new selection renders from the top.
    pub fn tick_review_watch(&mut self, now: Instant) -> bool {
        let Some(index) = self.review_index.as_mut() else {
            return false;
        };
        if self
            .review_scanned_at
            .is_some_and(|at| now.duration_since(at) < REVIEW_RESCAN_INTERVAL)
        {
            return false;
        }
        self.review_scanned_at = Some(now);
        let changed = index.rescan();
        if changed.is_empty() {
            return false;
        }
        let selected = self
            .tree
            .selected()
            .filter(|n| n.kind == NodeKind::File)
            .map(|n| n.path);
        let shown = self
            .active_display
            .displayed_origin()
            .map(|o| o.absolute_path().to_path_buf());
        match selected {
            Some(path) if Some(&path) == shown.as_ref() => {
                if changed.contains(&path) {
                    let mode = self.effective_mode(&path);
                    self.dispatch_reflow(path, mode);
                }
            }
            _ => self.dispatch_render(),
        }
        true
    }
}
