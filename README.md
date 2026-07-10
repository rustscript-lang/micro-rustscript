# micro-rustscript

[![rustscript-embedded on crates.io](https://img.shields.io/crates/v/rustscript-embedded.svg)](https://crates.io/crates/rustscript-embedded)

A small RustScript runner for embedded-facing host and constrained-target experiments. The published crate remains `rustscript-embedded`; the host examples use `pd-vm` with `default-features = false`, only the `runtime` feature, and JIT disabled.

## Host examples

```bash
cargo run --example run_file -- programs/blinky.rss
printf 'print(1 + 2);\n.quit\n' | cargo run --example repl
```

## Raspberry Pi Zero bare-metal scenario

The board scenario targets Raspberry Pi Zero revision 1.2: BCM2835 with ARM1176JZF-S, booted without an OS under QEMU's `raspi0` machine.

```bash
ci/run-qemu-raspi0.sh
ci/measure-size.sh
```

The firmware in `ports/raspi-zero/` contains a tiny frozen-program interpreter for a RustScript-flavored bytecode form of `programs/blinky.rss`. It prints over the BCM2835 PL011 UART and has no JIT path. This board smoke target measures the frozen bytecode firmware only; it does not represent a full `pd-vm` firmware image.

## CI shape borrowed from MicroPython

MicroPython keeps a central `tools/ci.sh`, has per-port workflows, runs QEMU ports across ARM/RISC-V boards, and has a separate code-size workflow based on `tools/metrics.py`. This project mirrors that shape at a small scale:

- host job: format, clippy, tests, native example smoke
- target job: install `arm-none-eabi` + QEMU, build the Raspberry Pi Zero bare-metal firmware, run it under QEMU
- size step: report ELF section footprint and raw image bytes

## Size controls

Host `min-size` profile uses:

- `opt-level = "z"`
- `lto = "fat"`
- `codegen-units = 1`
- `panic = "abort"`
- `strip = "symbols"`

The Pi Zero firmware uses `-Os`, `-ffunction-sections`, `-fdata-sections`, `-nostdlib`, and linker `--gc-sections`.
