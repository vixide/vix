#!/usr/bin/env bash
# Regenerate demo.sqlite from seed.sql. Requires the sqlite3 CLI.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
rm -f demo.sqlite
sqlite3 demo.sqlite < seed.sql
echo "wrote $(pwd)/demo.sqlite"
