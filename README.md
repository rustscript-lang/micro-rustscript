# micro-rustscript

[![rustscript-embedded on crates.io](https://img.shields.io/crates/v/rustscript-embedded.svg)](https://crates.io/crates/rustscript-embedded)

A small RustScript runner for embedded and constrained targets. The published Cargo package remains
`rustscript-embedded`; `micro-rustscript` is the project name used in public documentation.

## Host examples

The default `host` feature uses the complete `pd-vm` compiler and interpreter with JIT disabled:

```bash
cargo run --example run_file -- programs/blinky.rss
printf 'print(1 + 2);\n.quit\n' | cargo run --example repl
```

## RP2040 / Raspberry Pi Pico

The RP2040 integration links the sibling `pd-vm-nostd` crate. Only the VMBC decoder, compact
interpreter, synchronous host callbacks, and instruction fuel compile for `thumbv6m-none-eabi`;
the source compiler, CLI, debugger, and JIT remain in host-side `pd-vm`.

RustScript source is compiled to VMBC during the PlatformIO build. The firmware links the real Rust
`no_std + alloc` interpreter as a static library and runs it through Arduino-Pico callbacks for GPIO,
delay, serial output, and allocation.

```bash
rustup target add thumbv6m-none-eabi
uv tool install platformio
pio run -d platformio/rp2040
```

The generated firmware files are under:

```text
platformio/rp2040/.pio/build/pico/firmware.elf
platformio/rp2040/.pio/build/pico/firmware.uf2
```

The verified PlatformIO build reports 87,908 flash bytes and 12,668 RAM bytes. The UF2 container is
200,192 bytes. These figures include Arduino-Pico, `pd-vm-nostd`, the allocator bridge, host
callbacks, and the embedded 780-byte VMBC program.

## Integration shape

- `platformio/rp2040/programs/blinky.rss`: host-compiled RustScript source
- `platformio/rp2040/scripts/build_rust.py`: Cargo + VMBC pre-build integration
- `platformio/rp2040/src/main.cpp`: Arduino GPIO, delay, serial, allocator, and host callback bridge
- `include/rustscript_embedded.h`: C ABI for the Rust static library

Applications may replace `blinky.rss` and extend the C callback dispatcher without adding SoC or
Arduino dependencies to `pd-vm-nostd`.

## Raspberry Pi Zero note

The older `ports/raspi-zero` files are a frozen-bytecode board smoke target. They do not contain
`pd-vm` or `pd-vm-nostd` and must not be used as evidence of a complete RustScript bare-metal
runtime. Use the RP2040 integration for the maintained `no_std` path.

## Size profile

Release and min-size builds use:

- `opt-level = "z"`
- `lto = "fat"`
- `codegen-units = 1`
- `panic = "abort"`
- `strip = "symbols"`
