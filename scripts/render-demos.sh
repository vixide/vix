#!/usr/bin/env bash
# Regenerate every demo GIF under docs/demos/ from its .tape script
# (improvement plan T406, tasks.md).
#
# Prerequisites: vhs <https://github.com/charmbracelet/vhs>, which in turn
# needs ttyd and a headless-capable Chrome/Chromium on PATH -- on macOS/
# Linux, `brew install vhs` pulls in ttyd and ffmpeg as formula
# dependencies (Chrome/Chromium is expected to already be installed
# separately). See vhs's own README for other platforms.
#
# Not part of CI or `scripts/check`: recording a real terminal session
# needs a real (or headless-capable) display and is comparatively slow --
# run this by hand after a change that affects what a demo shows, review
# the refreshed GIF(s), and commit them.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

if ! command -v vhs >/dev/null 2>&1; then
	echo "vhs not found on PATH -- install it first: brew install vhs" >&2
	echo "(or see https://github.com/charmbracelet/vhs#installation)" >&2
	exit 1
fi

echo "==> cargo build --release"
cargo build --release
export PATH="$PWD/target/release:$PATH"

count=0
for tape in docs/demos/*.tape; do
	echo "==> vhs $tape"
	vhs "$tape"
	count=$((count + 1))
done

echo "OK: regenerated $count demo GIF(s) from docs/demos/*.tape"
