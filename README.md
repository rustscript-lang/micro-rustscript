# micro-rustscript

[![rustscript-embedded on crates.io](https://img.shields.io/crates/v/rustscript-embedded.svg)](https://crates.io/crates/rustscript-embedded)

A small RustScript VMBC runner for embedded and constrained targets.

## Host examples

The default `host` feature uses the complete `pd-vm` compiler and interpreter with JIT disabled:

```bash
cargo run --example run_file -- programs/blinky.rss
printf 'print(1 + 2);\n.quit\n' | cargo run --example repl
```

## Raspberry Pi Pico / RP2040

The RP2040 integration uses the `pd-vm` `embedded-runtime` feature from the dedicated
`feat/no-std-runtime` branch. Only the VMBC decoder, compact interpreter, synchronous host
callbacks, and instruction fuel compile for `thumbv6m-none-eabi`; the compiler, CLI, debugger,
and JIT stay on the host.

RustScript source is compiled to VMBC during the PlatformIO build. The firmware links the real
Rust `no_std + alloc` interpreter as a static library and runs it through Arduino-Pico callbacks
for GPIO, delay, and serial output.

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

The verified clean-checkout PlatformIO build reports 87,940 flash bytes and 12,668 RAM bytes. The
UF2 container is 200,192 bytes. These figures include Arduino-Pico, the RustScript VMBC decoder and
interpreter, the allocator bridge, host callbacks, and the embedded 780-byte VMBC program.

## Integration shape

- `platformio/rp2040/programs/blinky.rss`: host-compiled RustScript source
- `platformio/rp2040/scripts/build_rust.py`: Cargo + VMBC pre-build integration
- `platformio/rp2040/src/main.cpp`: Arduino GPIO, delay, serial, allocator, and host callback bridge
- `include/rustscript_embedded.h`: stable C ABI for the Rust static library

Applications may replace `blinky.rss` and extend the C callback dispatcher without adding SoC
register or Arduino dependencies to `pd-vm` itself.

## Size profile

Release and min-size builds use:

- `opt-level = "z"`
- `lto = "fat"`
- `codegen-units = 1`
- `panic = "abort"`
- stripped Rust symbols for release artifacts
