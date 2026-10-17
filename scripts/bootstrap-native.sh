#!/usr/bin/env bash
set -euo pipefail
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [[ "$(uname -sm)" != "Darwin arm64" ]]; then
  echo "This bootstrap supports Apple Silicon macOS." >&2
  exit 1
fi
if [[ -x "$PROJECT_ROOT/.tools/zig/zig" ]]; then
  "$PROJECT_ROOT/.tools/zig/zig" version
  exit 0
fi
mkdir -p "$PROJECT_ROOT/.tools"
ARCHIVE="$PROJECT_ROOT/.tools/zig-0.16.0.tar.xz"
curl -fL --retry 2 https://ziglang.org/download/0.16.0/zig-aarch64-macos-0.16.0.tar.xz -o "$ARCHIVE"
echo "b23d70deaa879b5c2d486ed3316f7eaa53e84acf6fc9cc747de152450d401489  $ARCHIVE" | shasum -a 256 -c -
tar -xJf "$ARCHIVE" -C "$PROJECT_ROOT/.tools"
ln -sfn zig-aarch64-macos-0.16.0 "$PROJECT_ROOT/.tools/zig"
"$PROJECT_ROOT/.tools/zig/zig" version
