# RustScript framework API

The VM calls platform functions through static host-import names such as `gpio::digital_write`.
RSS programs import the built-in modules they use:

```rust
use gpio;
use mcu;
use serial;
use wifi;
use bluetooth;

let ready: bool = gpio::configure(8, 1);
let written: bool = gpio::digital_write(8, ready);
mcu::delay_ms(100);
serial::write_line("ready");
```

The compiler knows the module declarations, while each firmware target decides which imports it can
dispatch. Calling a function that is absent on the selected target fails host dispatch. Use the
support matrix below when writing a portable program.

## Target support

| Module or function | ESP32-C3 | ESP32-S31 preview | Arduino host simulator |
|---|:---:|:---:|:---:|
| `gpio::configure` | yes | yes | yes |
| `gpio::digital_write` | yes | yes | yes |
| `gpio::digital_read` | yes | yes | yes |
| `gpio::analog_read` | yes | — | — |
| `gpio::pwm_write` | yes | — | — |
| all `i2c` functions | yes | — | — |
| `mcu::delay_ms` | yes | yes | yes |
| `mcu::millis`, `mcu::micros` | yes | yes | — |
| `mcu::free_heap`, `mcu::random` | yes | yes | — |
| `mcu::delay_us`, `mcu::cpu_frequency_mhz`, `mcu::flash_size` | yes | — | — |
| `mcu::restart`, `mcu::deep_sleep_us` | yes | — | — |
| `serial::write_line` | yes | yes | yes |
| `serial::available`, `serial::read_bytes` | yes | — | — |
| all `wifi` functions | feature-gated | feature-gated | — |
| all `bluetooth` functions | feature-gated | feature-gated | — |

The release ESP targets enable both `wifi` and `bluetooth`. They are independent Cargo and
PlatformIO features. Removing one from `custom_rust_features` removes that module's host exports
from the firmware. The Arduino host simulator never registers either module.

## Return and error behavior

The signatures below are RSS signatures. Argument type, arity, and range checks happen in the host
bridge. A rejected call returns a host-dispatch error to the VM. A `bool` result reports whether the
underlying platform operation was accepted. Calls documented as `null` are commands whose success
is represented by normal completion.

## GPIO

```rust
use gpio;

gpio::configure(pin: int, mode: int) -> bool
gpio::digital_write(pin: int, high: bool) -> bool
gpio::digital_read(pin: int) -> bool
gpio::analog_read(pin: int) -> int
gpio::pwm_write(pin: int, duty: int, frequency: int, resolution_bits: int) -> bool
```

`pin` must be inside the SoC GPIO range. ESP32-C3 modes are `0` input, `1` output, `2` input
pull-up, `3` input pull-down, and `4` open-drain output. ESP32-S31 currently accepts modes `0` and
`1`. The simulator forwards the integer mode to its Arduino compatibility layer.

ESP32-C3 PWM supports six simultaneously assigned pins. Frequency is `1..40,000,000` Hz,
resolution is `1..16` bits, and duty must fit the selected resolution.

## I2C

ESP32-C3 only:

```rust
use i2c;

i2c::open(sda: int, scl: int, frequency: int) -> bool
i2c::close() -> null
i2c::transmit(address: int, data: bytes) -> int
i2c::transmit_register(address: int, register: int, data: bytes) -> int
i2c::receive(address: int, length: int) -> bytes
i2c::receive_register(address: int, register: int, length: int) -> bytes
```

Addresses must be 7-bit device addresses in `0x08..0x77`. Frequency must be
`1,000..5,000,000` Hz. Register values are `0..255`; payloads and reads are limited to 255 bytes.
Transmit functions return the Arduino Wire status code. A failed register-address phase returns an
empty byte sequence.

## MCU

```rust
use mcu;

mcu::delay_ms(duration: int) -> null
mcu::delay_us(duration: int) -> null
mcu::millis() -> int
mcu::micros() -> int
mcu::cpu_frequency_mhz() -> int
mcu::free_heap() -> int
mcu::flash_size() -> int
mcu::random() -> int
mcu::restart() -> null
mcu::deep_sleep_us(duration: int) -> null
```

On ESP32-C3, `delay_ms` accepts `0..60,000`, `delay_us` accepts `0..1,000,000`, and deep sleep
accepts `1..86,400,000,000` microseconds. ESP32-S31 `delay_ms` accepts a non-negative 32-bit
millisecond value. `millis` and `micros` are monotonic uptime counters. `restart` and
`deep_sleep_us` do not return after the platform action succeeds.

## Serial

```rust
use serial;

serial::write_line(value: string) -> null
serial::available() -> int
serial::read_bytes(maximum: int) -> bytes
```

`write_line` appends a newline. ESP32-C3 input uses the 115200-baud primary serial port;
`read_bytes` is non-blocking and returns at most 255 currently buffered bytes. ESP32-S31 and the
simulator currently expose output only.

## Wi-Fi

Available on ESP targets when the `wifi` feature is enabled:

```rust
use wifi;

wifi::connect(ssid: string, password: string) -> bool
wifi::disconnect() -> bool
wifi::is_connected() -> bool
wifi::rssi() -> int
wifi::local_ip() -> string
```

SSID length is `1..32` bytes; password length is `0..64` bytes. An empty password requests an open
network. `connect` initializes the ESP-IDF station stack and returns after ESP-IDF accepts or rejects
the asynchronous connection request. Poll `is_connected` before using connection-dependent data.
`rssi` returns `-127` when no access point is available. `local_ip` returns an empty string until a
non-zero station address exists. `disconnect` returns `false` when Wi-Fi was never initialized or
ESP-IDF rejects the request.

## Bluetooth LE controller

Available on ESP targets when the `bluetooth` feature is enabled:

```rust
use bluetooth;

bluetooth::enable() -> bool
bluetooth::disable() -> bool
bluetooth::is_enabled() -> bool
```

This API controls the ESP-IDF BLE controller lifecycle only. `enable` initializes the controller if
needed and enables BLE mode. `disable` disables and deinitializes it. Both operations are idempotent.
No GAP, GATT, advertising, scanning, pairing, or characteristic API is exposed yet.

## C host ABI

The Rust static library exports `rustscript_run_vmbc` and value types from
`include/rustscript_embedded.h`. Platform bridges provide a callback with this logical contract:

```text
(context, import_name_bytes, arguments, result) -> status
```

A callback status of `-1` rejects the call, `0` completes a command with `null`, and `1` returns the
value written to `result`. String and byte results reference bridge-owned storage and are consumed by
the VM during the call. Platform implementations are located in:

- `firmware/host_framework.cpp` — ESP32-C3 Arduino/ESP-IDF bridge.
- `esp32s31/main/main.cpp` — ESP32-S31 pure ESP-IDF bridge.
- `firmware/arduino/main.cpp` — native Arduino compatibility simulator.
