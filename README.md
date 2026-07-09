# rustscript-embedded

Embedded-facing RustScript runner samples. The crate uses `pd-vm` with `default-features = false` and only the `runtime` feature, so the CLI, protocol host layers, and Cranelift JIT dependencies are left out. Runtime instances are created with `JitConfig { enabled: false, .. }`.

## Examples

```bash
cargo run --example run_file -- programs/blinky.rss
printf 'print(1 + 2);\n.quit\n' | cargo run --example repl
```

## Target used for emulation

The project targets `aarch64-unknown-linux-gnu`, matching a Raspberry Pi 3/4 class Cortex-A SoC Linux environment. CI and local checks run the cross-built binaries under `qemu-aarch64 -L /usr/aarch64-linux-gnu`.

```bash
ci/run-qemu-aarch64.sh
ci/measure-size.sh
```

## CI shape borrowed from MicroPython

MicroPython keeps a central `tools/ci.sh`, has per-port workflows, runs QEMU ports across ARM/RISC-V boards, and has a separate code-size workflow based on `tools/metrics.py`. This project mirrors that shape at a small scale:

- host job: format, clippy, tests, native example smoke
- target job: install cross compiler + QEMU, build `aarch64-unknown-linux-gnu`, run examples under QEMU
- size job step: report ELF section footprint for the min-size profile

## Size controls

The `min-size` profile uses:

- `opt-level = "z"`
- `lto = "fat"`
- `codegen-units = 1`
- `panic = "abort"`
- `strip = "symbols"`

Those combine with disabled default features to reduce the runtime binary surface.
