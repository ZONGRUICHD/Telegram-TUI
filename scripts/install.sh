#!/usr/bin/env bash
# Source installer. Run from a checkout; never pipe this script into a shell.
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
prefix="${INSTALL_DIR:-$HOME/.local/lib/telegram-tui}"
cd "$root"
command -v cargo >/dev/null || { echo 'Install Rust first; see docs/BUILD.md' >&2; exit 1; }
library=libtdjson.so
[[ "$(uname -s)" != Darwin ]] || library=libtdjson.dylib
[[ -f "native/$library" ]] || {
  echo 'Build the pinned TDLib first: bash scripts/build-tdlib.sh' >&2; exit 1;
}
cargo build --workspace --release --locked
metadata=$(LIBTDJSON_PATH="$root/native/$library" ./target/release/tgcd --check-library)
printf '%s\n' "$metadata"
command -v python3 >/dev/null || { echo 'python3 is required to verify TDLib revision' >&2; exit 1; }
printf '%s' "$metadata" | python3 -c 'import json,sys; v=json.load(sys.stdin); assert v["commit"] == v["expected_commit"], "Wrong TDLib revision"'
mkdir -p "$prefix"
cp target/release/tg target/release/tgcd "$prefix/"
cp native/* "$prefix/"
cp README.md LICENSE TDLIB_COMMIT THIRD_PARTY_NOTICES.md "$prefix/"
cp -R docs "$prefix/"
printf 'Installed to %s\nAdd this directory to PATH, then run tg doctor and tg login.\n' "$prefix"
