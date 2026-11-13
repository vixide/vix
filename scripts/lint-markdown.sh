#!/bin/sh
# Proofread the repository's Markdown files.
#
#     scripts/lint-markdown.sh check [--quiet]
#
# `check` runs scripts/check-docs (spelling with cspell, internal links and
# the other documentation checks), then markdownlint-cli2 when it is already
# installed. Nothing is downloaded: without markdownlint-cli2 on PATH that
# step says so and is skipped. `--quiet` prints a step's output only when it
# fails. Exits non-zero when any step fails. Invoked via `make lint-markdown`.
set -u

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1

usage() {
	echo "usage: scripts/lint-markdown.sh check [--quiet]" >&2
	exit 2
}

[ "$#" -ge 1 ] && [ "$1" = check ] || usage
shift
quiet=no
for arg in "$@"; do
	case "$arg" in
	--quiet) quiet=yes ;;
	*) usage ;;
	esac
done

log=$(mktemp)
trap 'rm -f "$log"' EXIT
failed=0

# step NAME COMMAND...: run COMMAND, showing its output unless --quiet and it
# passed.
step() {
	name=$1
	shift
	if [ "$quiet" = yes ]; then
		if "$@" >"$log" 2>&1; then
			return 0
		fi
		echo "FAIL: $name" >&2
		cat "$log" >&2
	else
		echo "==> $name"
		"$@" && return 0
		echo "FAIL: $name" >&2
	fi
	failed=1
}

step "scripts/check-docs" python3 scripts/check-docs

if command -v markdownlint-cli2 >/dev/null 2>&1; then
	step "markdownlint-cli2" markdownlint-cli2 "**/*.md" \
		"#**/node_modules/**" "#**/target/**" "#.claude/**" "#**/build/**"
elif [ "$quiet" = no ]; then
	echo "==> markdownlint-cli2 (skipped: not installed)"
fi

exit "$failed"
