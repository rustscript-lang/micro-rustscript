# micro-rustscript

[![rustscript-embedded on crates.io](https://img.shields.io/crates/v/rustscript-embedded.svg)](https://crates.io/crates/rustscript-embedded)

A directly flashable RustScript runtime for ESP32-C3. The firmware combines Arduino-ESP32,
`pd-vm-nostd`, an embedded VMBC program, memory hooks, and board host functions into one factory
image. The published Cargo package remains `rustscript-embedded`.

## Flash the ready-made image

Download `micro-rustscript-esp32-c3.factory.bin` from the latest GitHub Release, connect an
ESP32-C3-DevKitM-1 compatible board, then write the complete image at offset zero:

```bash
python -m esptool --chip esp32c3 erase_flash
python -m esptool --chip esp32c3 write_flash 0x0 micro-rustscript-esp32-c3.factory.bin
```

After reset, the bundled RSS program configures GPIO 8, toggles it four times, reads it back, and
prints the result plus `micro-rustscript:status=0` at 115200 baud. The factory image already contains
the bootloader, partition table, Arduino boot component, Rust runtime, and VMBC payload.

## GPIO from RSS

RSS declares board capabilities as host imports. They are resolved by the firmware's exported host
function table:

```rust
fn gpio_mode(pin: int, mode: int);
fn gpio_write(pin: int, high: bool);
fn gpio_read(pin: int) -> bool;
fn delay_ms(milliseconds: int);
fn serial_write(message: string);

gpio_mode(8, 1);
gpio_write(8, true);
delay_ms(100);
let high = gpio_read(8);
```

Built-in exports:

| RSS function | Behavior |
|---|---|
| `gpio_mode(pin, mode)` | Configure input (`0`), output (`1`), pull-up (`2`), or pull-down (`3`) |
| `gpio_write(pin, high)` | Set a digital output |
| `gpio_read(pin)` | Read a digital level and return `bool` to RSS |
| `delay_ms(milliseconds)` | Pause execution for up to 60 seconds |
| `serial_write(message)` | Write a line to the serial monitor |

`firmware/main.cpp` owns the host export table. Add another table entry and handler to expose SPI,
I2C, PWM, sensors, or application services without changing `pd-vm-nostd`. Host handlers may return
void, a scalar value, or an error through the C ABI in `include/rustscript_embedded.h`.

## Build a custom image

The repository root is a complete PlatformIO project. Source compilation, Rust static-library
linking, VMBC embedding, ESP32-C3 firmware generation, and factory-image merging all run from one
command. `rust-toolchain.toml` pins the Rust toolchain and installs the RISC-V target plus
`llvm-tools-preview`; Cargo pins the matching RustScript VM revision, so no adjacent repository
checkout or manual archive copy is needed. Only Rustup and PlatformIO are required:

```bash
uv tool install platformio
pio run
```

Replace `programs/esp32-blinky.rss` to bundle another script. Build outputs include:

```text
.pio/build/esp32-c3-devkitm-1/firmware.elf
.pio/build/esp32-c3-devkitm-1/firmware.bin
dist/micro-rustscript-esp32-c3.factory.bin
```

The VMBC source compiler runs on the build machine. The ESP32-C3 image contains the compact
`no_std + alloc` decoder and interpreter from `pd-vm-nostd`; compiler, CLI, debugger, and JIT code
are excluded from the device image.

## Host-side examples

The default `host` feature still provides desktop runner samples:

```bash
cargo run --example run_file -- programs/blinky.rss
printf 'print(1 + 2);\n.quit\n' | cargo run --example repl
```

## Size profile

Release builds use size optimization, fat LTO, one codegen unit, abort-on-panic, and symbol
stripping. GitHub Releases include the factory image and SHA-256 checksums.
