#!/usr/bin/env bash
# Launcher for the review sidecar (the `open-review-sidecar` action). "Launch-or-focus, toggle on
# repeat", scoped to the current tab, exactly like open-file-viewer.sh but for the pane titled
# "Review" (`--launch-decision-review`), so it never touches a file viewer's "Files" pane:
#   - no Review pane in the current tab      -> open a split (focused)
#   - a Review pane exists but isn't focused  -> focus it
#   - the focused pane IS the Review pane     -> close it
#
# On OPEN the review directory is asked of the binary (`--review-root`), which derives it from the
# herdr workspace name and the focused pane's git checkout. The directory is created if it does not
# exist yet, so the sidecar can be opened before the first item is written; the running sidecar
# picks items up as they appear. When no directory can be named, a herdr notification says why and
# no pane opens.
set -uo pipefail

herdr_bin="${HERDR_BIN_PATH:-herdr}"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
viewer_bin="$script_dir/../target/release/herdr-file-viewer"
plugin_id="giggs.review-sidecar"

fail() {
  echo "review sidecar: $1" >&2
  "$herdr_bin" notification show "Review sidecar" --body "$1" >/dev/null 2>&1 || true
  exit 1
}

open_pane() {
  [ -x "$viewer_bin" ] || fail "the viewer binary is not built: $viewer_bin"
  local dir
  dir="$("$viewer_bin" --review-root 2>&1)" || fail "${dir#review sidecar: }"
  mkdir -p -- "$dir" || fail "cannot create $dir"
  exec "$herdr_bin" plugin pane open \
    --plugin "$plugin_id" \
    --entrypoint review \
    --placement split \
    --direction right \
    --env "HERDR_FILE_VIEWER_ROOT=$dir" \
    --focus
}

decision="OPEN"
if [ -x "$viewer_bin" ]; then
  panes="$("$herdr_bin" pane list 2>/dev/null || true)"
  if [ -n "$panes" ]; then
    decision="$(printf '%s' "$panes" | "$viewer_bin" --launch-decision-review 2>/dev/null || echo OPEN)"
  fi
fi

case "$decision" in
  "FOCUS "*)
    pid="${decision#FOCUS }"
    "$herdr_bin" pane zoom "$pid" --on >/dev/null 2>&1 || true
    exec "$herdr_bin" pane zoom "$pid" --off
    ;;
  "CLOSE "*)
    pid="${decision#CLOSE }"
    exec "$herdr_bin" pane close "$pid"
    ;;
  *)
    open_pane
    ;;
esac
