#!/usr/bin/env bash
# Regenerate every demo GIF under docs/demos/ from its .tape script,
# without a headless browser (improvement plan T406, tasks.md).
#
# `scripts/render-demos.sh` (real VHS) is the documented, primary way to do
# this -- use it if your machine can run headless Chrome/Chromium. This
# script is for the opposite case: an environment whose headless browser is
# killed outright (see scripts/vhs_lite.py's own docstring for the full
# story) and so cannot run VHS's screenshot-based pipeline at all. It
# produces the same docs/demos/*.gif outputs via a different pipeline that
# never touches a browser: a real pty (pexpect) drives Vix, a hand-written
# asciicast v2 recorder captures it, and agg
# <https://github.com/asciinema/agg> (pure-Rust font rendering) turns that
# into the GIF.
#
# Prerequisites:
#   - Python 3 with `pexpect` (pip install --user pexpect)
#   - `agg` on PATH -- crates.io's "agg" is a different, unrelated crate
#     (Anti-Grain Geometry), so install asciinema's from source:
#       git clone https://github.com/asciinema/agg /tmp/agg-src
#       cargo install --path /tmp/agg-src
#
# Not part of CI or `scripts/check`, same as scripts/render-demos.sh: run
# this by hand after a change that affects what a demo shows, review the
# refreshed GIF(s), and commit them.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

if ! command -v agg >/dev/null 2>&1; then
	echo "agg not found on PATH -- see this script's header for how to build it" >&2
	exit 1
fi
if ! python3 -c "import pexpect" >/dev/null 2>&1; then
	echo "pexpect not importable -- install it: pip install --user pexpect" >&2
	exit 1
fi

echo "==> cargo build --release"
cargo build --release
export PATH="$PWD/target/release:$PATH"

# overview.tape and themes.tape both edit examples/demo-workspace/rust-app/
# src/main.rs; overview.tape saves its edit to disk, so themes.tape (which
# runs after it, alphabetically) would otherwise open the file mid-batch
# already carrying that edit. Reset the whole demo workspace first so every
# tape starts from its own clean, committed state, run order notwithstanding.
git checkout -- examples/demo-workspace

count=0
for tape in docs/demos/*.tape; do
	out_gif=$(grep -m1 '^Output ' "$tape" | awk '{print $2}')
	cast="$(mktemp -t vhs-lite-XXXXXX).cast"
	echo "==> $tape -> $cast"
	python3 scripts/vhs_lite.py "$tape" "$cast"
	echo "==> agg $cast -> $out_gif"
	agg "$cast" "$out_gif"
	rm -f "$cast"
	count=$((count + 1))
done

echo "OK: regenerated $count demo GIF(s) from docs/demos/*.tape (browser-free pipeline)"
