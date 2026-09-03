#!/usr/bin/env sh
#
# Regenerates the third-party license report with cargo-about and fails
# the run when generation (or the underlying license policy in
# about.toml) is violated.
#
# The report is written to a temp file; its path is printed on success.
# The temp file is removed on failure so a stale report is never
# mistaken for a fresh one.

set -eu

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
out="${TMPDIR:-/tmp}/telos-licenses-$(date +%s).json"

if cargo about generate --fail --format json -c "$repo_root/about.toml" >"$out"; then
    echo "license report generated at: $out"
else
    status=$?
    echo "cargo about generate failed; no report written" >&2
    rm -f "$out"
    exit "$status"
fi
