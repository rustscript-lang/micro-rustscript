# micro-rustscript

[![rustscript-embedded on crates.io](https://img.shields.io/crates/v/rustscript-embedded.svg)](https://crates.io/crates/rustscript-embedded)

A small RustScript runner for embedded and constrained targets. The published Cargo package remains
`rustscript-embedded`; `micro-rustscript` is the project name used in public documentation.

## Host examples

The default branch keeps host-side runner examples with JIT disabled:

```bash
cargo run --example run_file -- programs/blinky.rss
printf 'print(1 + 2);\n.quit\n' | cargo run --example repl
```

## RP2040 / Raspberry Pi Pico

The first real MCU integration is developed on
[`feat/rp2040-platformio`](https://github.com/rustscript-lang/rustscript-embedded/tree/feat/rp2040-platformio).
It links the `pd-vm` `no_std + alloc` VMBC interpreter into an Arduino-Pico firmware through a narrow
C ABI. RustScript source is compiled to VMBC on the host; GPIO, delay, serial output, and allocation
remain owned by the PlatformIO application.

```bash
git switch feat/rp2040-platformio
rustup target add thumbv6m-none-eabi
uv tool install platformio
pio run -d platformio/rp2040
```

That branch builds a real ELF and UF2 containing `rustscript_run_vmbc`, the compact interpreter, and
the embedded VMBC program. CI publishes both firmware artifacts.

## Raspberry Pi Zero note

The older `ports/raspi-zero` files on the default branch are a frozen-bytecode board smoke target.
They do not contain `pd-vm` and must not be used as evidence of a full RustScript bare-metal runtime.
Use the RP2040 branch for the maintained `no_std` integration.

## Size controls

Release profiles use:

- `opt-level = "z"`
- `lto = "fat"`
- `codegen-units = 1`
- `panic = "abort"`
- `strip = "symbols"`
