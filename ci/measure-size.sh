#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export CARGO_TARGET_DIR="$ROOT/target"

TARGET="${TARGET:-aarch64-unknown-linux-gnu}"
EXAMPLE="${EXAMPLE:-run_file}"
PROFILE="${PROFILE:-min-size}"
BIN="target/${TARGET}/${PROFILE}/examples/${EXAMPLE}"

rustup target add "$TARGET" >/dev/null
cargo build --profile "$PROFILE" --target "$TARGET" --example "$EXAMPLE"

if command -v aarch64-linux-gnu-size >/dev/null && [[ "$TARGET" == "aarch64-unknown-linux-gnu" ]]; then
  SIZE_TOOL=aarch64-linux-gnu-size
else
  SIZE_TOOL=size
fi

printf 'binary=%s\n' "$BIN"
"$SIZE_TOOL" "$BIN"
printf '\nsections:\n'
"$SIZE_TOOL" -A "$BIN" | sed -n '1,40p'
