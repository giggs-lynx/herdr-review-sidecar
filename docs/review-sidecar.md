# Review sidecar

This repository is a fork of [herdr-file-viewer](https://github.com/smarzban/herdr-file-viewer).
It ships the viewer as a **review sidecar**: a herdr split pane that lists review items — findings
from a code review, open questions from a design discussion — beside the work they are about, and
renders the selected one. The plugin id is `giggs.review-sidecar` and its pane is titled `Review`,
so it installs alongside herdr-file-viewer without either launcher touching the other's pane.

## Review items

A review directory holds one Markdown file per item. Every `.md` file in it that is not hidden is
listed; review items are named `F01.md`, `Q01.md`, `Q12.md` and start with a frontmatter block:

```markdown
---
title: Which layer owns the rule
status: open
---
The body, in markdown.
```

- `status` is `open`, `done` or `dropped`; anything else, or no status line, reads as `open`.
- A file with no `title:` is titled by its first `# ` heading; a file with no frontmatter at all has
  no status, so its row shows no glyph and is never dimmed.
- Hidden files, other file types and subdirectories are not listed.

## What the pane shows

- **Left:** one row per item, sorted by file name: `○ F01  title` for open items, `✓` for done and
  `✗` for dropped. Done and dropped rows are dimmed.
- **Right:** the selected item rendered as markdown, with its frontmatter replaced by a
  `# <id> <title>` heading. `v` still switches to the file's source.
- **Live:** the directory is checked once a second. New items appear, removed items disappear, and
  an edit to the item on screen re-renders it without losing your scroll position.

Everything else works as in the file viewer: click a row to select it, drag across the content to
select text, and release to copy it to the clipboard (OSC 52). See [Keys & mouse](keys.md).

Review mode never reads git and shows no remote update notices.

## Where items live

Each piece of work gets one directory under `~/.cache/sidecar/`, by the first rule that applies:

1. an issue key such as `ABC-123` in the herdr workspace name → `~/.cache/sidecar/ABC-123/`;
2. an issue key in the git branch name → `~/.cache/sidecar/ABC-123/`;
3. otherwise `~/.cache/sidecar/<repo>/<branch>/`, where `<repo>` is the main repository's
   directory name, shared by all its worktrees.

The workspace name comes first because an agent's working directory often sits in a main checkout
on some other branch, while the workspace is named for the work. Whoever writes items asks the same
binary for the directory, so writer and sidecar always agree:

```sh
cd <the work's checkout> && <plugin root>/target/release/herdr-file-viewer --review-root
```

It reads the workspace from `$HERDR_WORKSPACE_ID`, so run it inside the herdr pane of that work.

## Setup

1. Install or link the plugin (`herdr plugin link <path-to-this-checkout>` for a local checkout),
   and build it (`cargo build --release`).
2. Bind the action in herdr's `config.toml`:

   ```toml
   [[keys.command]]
   key = "alt+shift+f"
   type = "plugin_action"
   command = "giggs.review-sidecar.open-review-sidecar"
   description = "review sidecar"
   ```

The key opens the sidecar beside the focused pane, focuses it if it is already open in this tab,
and closes it when it is the focused pane. The directory is created if it does not exist yet, so
the sidecar can be opened before the first item is written. When no directory can be named (no
issue key in the workspace name and the focused pane is not in a git checkout), a herdr
notification says why and no pane opens.

## How it is wired

- The manifest pane runs the viewer with `--review`; the launcher hands the directory over with
  `plugin pane open --env HERDR_FILE_VIEWER_ROOT=<dir>`. Without a valid directory the pane shows
  the reason instead of falling back to the focused pane's cwd.
- `--review-root` prints the review directory; `--launch-decision-review` is the launcher's
  open/focus/close decision for the `Review` pane.
- The code lives in `src/review.rs` (item names, frontmatter, the directory index, where items
  live) and `src/controller/review.rs` (the once-a-second rescan).
