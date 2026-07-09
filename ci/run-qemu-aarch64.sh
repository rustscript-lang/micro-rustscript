#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export CARGO_TARGET_DIR="$ROOT/target"

rustup target add aarch64-unknown-linux-gnu >/dev/null
cargo build --target aarch64-unknown-linux-gnu --example run_file --example repl
cargo build --profile min-size --target aarch64-unknown-linux-gnu --example run_file --example repl

RUNNER=(qemu-aarch64 -L /usr/aarch64-linux-gnu)
"${RUNNER[@]}" target/aarch64-unknown-linux-gnu/debug/examples/run_file programs/blinky.rss | grep 'led:off'
printf 'print(1 + 2);\n.quit\n' | "${RUNNER[@]}" target/aarch64-unknown-linux-gnu/debug/examples/repl | grep '3'
