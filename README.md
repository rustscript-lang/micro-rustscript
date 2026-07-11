# micro-rustscript

[![rustscript-embedded on crates.io](https://img.shields.io/crates/v/rustscript-embedded.svg)](https://crates.io/crates/rustscript-embedded)

A directly flashable RustScript runtime for ESP32-C3. One image contains the bootloader,
partition table, Arduino-ESP32 runtime, `pd-vm-nostd`, framework host API, and a default VMBC
script partition.

## Flash the complete image

Download `micro-rustscript-esp32-c3.factory.bin` from the latest GitHub Release and flash it at
offset zero:

```bash
python -m esptool --chip esp32c3 erase_flash
python -m esptool --chip esp32c3 write_flash 0x0 micro-rustscript-esp32-c3.factory.bin
```

The boot order is fixed:

1. `/rustscript/main.vmbc` on an SD card connected with CS on GPIO 7.
2. The dedicated 64 KiB `rustscript` flash partition at `0x110000`.
3. The serial VMBC REPL at 115200 baud.

An absent, unreadable, or missing SD script automatically falls through to the flash partition.
The release factory image already contains `esp32-blinky.vmbc` in that partition.
`RUSTSCRIPT_SD_CS` and `RUSTSCRIPT_SD_PATH` can be overridden with PlatformIO build flags.

## Framework API from RSS

Hardware functions are exposed through RSS modules, keeping board ABI names private. Import only the
capabilities a script uses:

```rust
use framework::gpio as gpio;
use framework::i2c as i2c;
use framework::mcu as mcu;
use framework::serial as serial;
use framework::wifi as wifi;
use framework::bluetooth as bluetooth;

let ok: bool = gpio::configure(8, 1);
let written: bool = gpio::digital_write(8, true);
let level: bool = gpio::digital_read(8);

let bus_ready: bool = i2c::open(8, 9, 400000);
let status: int = i2c::transmit_register(0x3c, 0, b"hello");
let reply: bytes = i2c::receive_register(0x3c, 0, 8);
i2c::close();

mcu::delay_ms(100);
let free_heap: int = mcu::free_heap();
serial::write_line("ready");

let started: bool = wifi::connect("ssid", "password");
let address: string = wifi::local_ip();
let ble_ready: bool = bluetooth::enable();
```

### GPIO

| Function | Result |
|---|---|
| `gpio::configure(pin, mode)` | `bool`; modes: input `0`, output `1`, pull-up `2`, pull-down `3`, open-drain `4` |
| `gpio::digital_write(pin, high)` | `bool` |
| `gpio::digital_read(pin)` | `bool` |
| `gpio::analog_read(pin)` | ADC value as `int` |
| `gpio::pwm_write(pin, duty, frequency, resolution_bits)` | `bool`; six channels, 1–16 bits |

### I2C

| Function | Result |
|---|---|
| `i2c::open(sda, scl, frequency)` | `bool` |
| `i2c::close()` | `null` |
| `i2c::transmit(address, data)` | Wire status as `int` |
| `i2c::transmit_register(address, register, data)` | Wire status as `int` |
| `i2c::receive(address, length)` | Up to 255 bytes |
| `i2c::receive_register(address, register, length)` | Up to 255 bytes |

### MCU and serial

`mcu` exports `delay_ms`, `delay_us`, `millis`, `micros`, `cpu_frequency_mhz`, `free_heap`,
`flash_size`, `random`, `restart`, and `deep_sleep_us`. `serial` exports `write_line`, `available`,
and `read_bytes`.

### Wi-Fi and Bluetooth LE

The `wifi` API exports `connect`, `disconnect`, `is_connected`, `rssi`, and `local_ip`. `connect`
returns whether ESP-IDF accepted the asynchronous connection request; poll `is_connected` before
using `rssi` or `local_ip`. The `bluetooth` API exports BLE-controller lifecycle operations:
`enable`, `disable`, and `is_enabled`. Both use ESP-IDF APIs and are registered only on supported
ESP targets.

`wifi` and `bluetooth` are independent Cargo/PlatformIO features. ESP release targets enable both
by default through `custom_rust_features`; removing either feature also removes its ESP-IDF includes
and RSS host exports. The host `arduino` target exports neither API.

The private host ABI lives in `firmware/host_framework.cpp`; the public RSS modules live under
`programs/framework/`. This keeps script-facing APIs namespaced while allowing the VM to dispatch a
compact static function table.

## Replace only the VMBC partition

The application image remains intact when only the script partition is flashed. The helper accepts
an RSS source file or an already compiled VMBC file, derives the offset and capacity from
`partitions.csv`, adds a versioned length/CRC header, and calls esptool:

```bash
python scripts/flash_vmbc.py programs/my-app.rss --port /dev/ttyACM0
python scripts/flash_vmbc.py app.vmbc --port /dev/ttyACM0
```

Create a partition image without connecting a board:

```bash
python scripts/flash_vmbc.py app.vmbc --output app.partition.bin
```

RSS input compilation uses `rustscript-compile-vmbc` from `PATH`, or the compiler binary in this
checkout through Cargo. A raw `.vmbc` file can be copied directly to the SD path; SD files do not
use the flash-partition header.

## Serial VMBC REPL

When neither startup source exists, the firmware presents a VMBC-oriented serial REPL. Commands are
`load`, `install`, `run`, `info`, and `help`. `load` executes a transferred VMBC payload without
writing flash; `install` writes the script partition first. The helper implements the binary framing:

```bash
python -m pip install pyserial
python scripts/repl_vmbc.py app.vmbc --port /dev/ttyACM0
python scripts/repl_vmbc.py app.vmbc --port /dev/ttyACM0 --install
```

Source compilation stays on the development machine; the firmware image contains the decoder and
interpreter, without the desktop compiler.

## Build

The repository root is a complete PlatformIO project:

```bash
export UV_TOOL_DIR=/mnt/TEMP/platformio/tools
export UV_TOOL_BIN_DIR=/mnt/TEMP/platformio/bin
export UV_CACHE_DIR=/mnt/TEMP/platformio/uv-cache
export PLATFORMIO_CORE_DIR=/mnt/TEMP/platformio/core
uv tool install platformio==6.1.19
export PATH=/mnt/TEMP/platformio/bin:$PATH

pio run -e esp32-c3-devkitm-1
pio run -e arduino
ci/install-esp-idf-s31.sh
pio run -e esp32s31
.pio/build/arduino/program
```

Outputs:

```text
.pio/build/esp32-c3-devkitm-1/firmware.elf
.pio/build/esp32-c3-devkitm-1/firmware.bin
.pio/build/arduino/program
.pio/generated/esp32-blinky.vmbc
.pio/generated/rustscript.partition.bin
dist/micro-rustscript-esp32-c3.factory.bin
dist/micro-rustscript-esp32-s31.factory.bin
```

The factory image merges the ESP32 boot components, application, and default script partition. The
release includes the factory image, ELF, VMBC, packed script partition, flash helpers, partition CSV,
and SHA-256 checksums.

The `esp32s31` target uses pinned ESP-IDF master preview support. ESP-IDF source, Python environment,
toolchains, caches, Rust target artifacts, generated files, and build output are all kept under
`/mnt/TEMP`; only source and build configuration live in the repository.

The `arduino` environment links `pd-vm-nostd` through an Arduino-compatible GPIO, delay, serial,
and allocator bridge. It runs the bridge and compiled VMBC program on the host before a board is
connected. A successful simulation ends with `rss:status=0`.

## ESP32 image size

The ESP32 partition table uses a 1 MiB factory application slot and a 64 KiB VMBC slot. OTA data
and SPIFFS partitions are omitted because this image is flashed directly and script updates use the
dedicated VMBC partition. With `wifi` and `bluetooth` enabled, the measured factory image is
1,115,607 bytes, down from 2,164,183 bytes (48.45%), while retaining SD boot, the flash script,
and the serial VMBC REPL.
