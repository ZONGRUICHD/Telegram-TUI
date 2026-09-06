#!/usr/bin/env bash
# Build the exact TDLib revision used by Telegram-TUI. No global installation.
set -euo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
revision=$(tr -d '\r\n' < "$root/TDLIB_COMMIT")
work="${BUILD_DIR:-$root/td-build}"
native="${TDLIB_OUTPUT:-$root/native}"
for tool in git cmake gperf; do
  command -v "$tool" >/dev/null || { echo "Missing $tool; see docs/BUILD.md" >&2; exit 1; }
done
mkdir -p "$work" "$native"
if [[ ! -d "$work/source/.git" ]]; then
  git init "$work/source"
  git -C "$work/source" remote add origin https://github.com/tdlib/td.git
  git -C "$work/source" fetch --depth 1 origin "$revision"
  git -C "$work/source" checkout --detach FETCH_HEAD
fi
[[ "$(git -C "$work/source" rev-parse HEAD)" == "$revision" ]] || {
  echo "Existing TDLib source has a different revision; choose a new BUILD_DIR." >&2; exit 1;
}
options=(-DCMAKE_BUILD_TYPE=Release -DTD_ENABLE_LTO=OFF -DCMAKE_POLICY_VERSION_MINIMUM=3.5)
library=libtdjson.so
if [[ "$(uname -s)" == Darwin ]]; then
  library=libtdjson.dylib
  if command -v brew >/dev/null; then
    options+=(-DOPENSSL_ROOT_DIR="$(brew --prefix openssl@3)" -DOPENSSL_USE_STATIC_LIBS=TRUE)
  fi
elif command -v php >/dev/null; then
  if git -C "$work/source" apply --check --unidiff-zero "$root/scripts/tdlib-split-fix.patch" 2>/dev/null; then
    git -C "$work/source" apply --unidiff-zero "$root/scripts/tdlib-split-fix.patch"
  fi
  (cd "$work/source" && php SplitSource.php)
fi
cmake -S "$work/source" -B "$work/build" "${options[@]}"
cmake --build "$work/build" --target tdjson --parallel "${BUILD_JOBS:-2}"
cp "$work/build/$library" "$native/$library"
cp "$work/source/LICENSE_1_0.txt" "$native/TDLib-LICENSE.txt"
printf 'Built %s\nRun scripts/install.sh or set LIBTDJSON_PATH to this full path.\n' "$native/$library"
