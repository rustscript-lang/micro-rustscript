#![cfg(feature = "esp32c3")]

use std::ffi::c_void;
use std::slice;

use pd_vm_nostd::Value as NoStdValue;
use rustscript_embedded::{
    ReplTransport, ReplValue, RustScriptBuffer, RustScriptHostCallback, RustScriptValue,
    RustScriptValueTag, SerialReplSession, rustscript_buffer_free, rustscript_repl_run_vmbc,
    rustscript_run_vmbc,
};
use vm::{compile_source, compile_source_file, encode_program};

#[derive(Default, Debug, PartialEq, Eq)]
struct BoardState {
    pin: i64,
    mode: i64,
    high: bool,
    gpio_writes: usize,
    gpio_reads: usize,
    delayed_ms: i64,
    serial: Vec<u8>,
}

unsafe extern "C" fn host_callback(
    context: *mut c_void,
    name: *const u8,
    name_len: usize,
    args: *const RustScriptValue,
    arg_count: usize,
    result: *mut RustScriptValue,
) -> i32 {
    let state = unsafe { &mut *context.cast::<BoardState>() };
    let name = unsafe { slice::from_raw_parts(name, name_len) };
    let args = unsafe { slice::from_raw_parts(args, arg_count) };
    match name {
        b"gpio::configure"
            if args.len() == 2
                && args[0].tag == RustScriptValueTag::Int as u8
                && args[1].tag == RustScriptValueTag::Int as u8 =>
        {
            state.pin = args[0].integer;
            state.mode = args[1].integer;
            unsafe { set_bool_result(result, true) }
        }
        b"gpio::digital_write" | b"gpio_digital_write"
            if args.len() == 2
                && args[0].tag == RustScriptValueTag::Int as u8
                && args[1].tag == RustScriptValueTag::Bool as u8 =>
        {
            state.pin = args[0].integer;
            state.high = args[1].boolean != 0;
            state.gpio_writes += 1;
            unsafe { set_bool_result(result, true) }
        }
        b"gpio::digital_read" | b"gpio_digital_read"
            if args.len() == 1 && args[0].tag == RustScriptValueTag::Int as u8 =>
        {
            state.pin = args[0].integer;
            state.gpio_reads += 1;
            unsafe { set_bool_result(result, state.high) }
        }
        b"mcu::delay_ms" if args.len() == 1 && args[0].tag == RustScriptValueTag::Int as u8 => {
            state.delayed_ms += args[0].integer;
            0
        }
        b"serial::write_line"
            if args.len() == 1 && args[0].tag == RustScriptValueTag::String as u8 =>
        {
            if args[0].len != 0 && args[0].data.is_null() {
                return -1;
            }
            let bytes = unsafe { slice::from_raw_parts(args[0].data, args[0].len) };
            state.serial.extend_from_slice(bytes);
            0
        }
        _ => -1,
    }
}

unsafe fn set_bool_result(result: *mut RustScriptValue, value: bool) -> i32 {
    if result.is_null() {
        return -1;
    }
    unsafe {
        *result = RustScriptValue::null();
        (*result).tag = RustScriptValueTag::Bool as u8;
        (*result).boolean = u8::from(value);
    }
    1
}

fn compile_vmbc(source: &str) -> Vec<u8> {
    let compiled = compile_source(source).expect("source should compile");
    encode_program(&compiled.program.with_local_count(compiled.locals))
        .expect("program should encode")
}

#[test]
fn scalar_ffi_values_round_trip() {
    let values = [
        NoStdValue::Null,
        NoStdValue::Int(42),
        NoStdValue::Float(2.5),
        NoStdValue::Bool(true),
        NoStdValue::string("esp32"),
        NoStdValue::bytes([1, 2, 3]),
    ];

    for value in values {
        let ffi = RustScriptValue::from_embedded(&value).expect("scalar should convert");
        let decoded = unsafe { ffi.to_embedded() }.expect("FFI scalar should decode");
        assert_eq!(decoded, value);
    }
}

#[test]
fn c_abi_runs_vmbc_and_dispatches_host_call() {
    let bytes = compile_vmbc(
        r#"
            fn gpio_digital_write(pin: int, high: bool) -> bool;
            let ok: bool = gpio_digital_write(8, true);
        "#,
    );
    let mut state = BoardState::default();
    let callback: RustScriptHostCallback = host_callback;

    let status = unsafe {
        rustscript_run_vmbc(
            bytes.as_ptr(),
            bytes.len(),
            Some(callback),
            (&mut state as *mut BoardState).cast(),
            10_000,
        )
    };

    assert_eq!(status, 0);
    assert_eq!(state.pin, 8);
    assert!(state.high);
    assert_eq!(state.gpio_writes, 1);
}

#[test]
fn esp32_program_runs_through_real_ffi_path() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("programs/esp32-blinky.rss");
    let compiled = compile_source_file(&source).expect("ESP32 program should compile");
    let bytes = encode_program(&compiled.program).expect("program should encode");
    let mut state = BoardState::default();

    let status = unsafe {
        rustscript_run_vmbc(
            bytes.as_ptr(),
            bytes.len(),
            Some(host_callback),
            (&mut state as *mut BoardState).cast(),
            100_000,
        )
    };

    assert_eq!(status, 0);
    assert_eq!(state.pin, 8);
    assert_eq!(state.mode, 1);
    assert!(!state.high);
    assert_eq!(state.gpio_writes, 4);
    assert_eq!(state.gpio_reads, 1);
    assert_eq!(state.delayed_ms, 400);
    assert_eq!(state.serial, b"micro-rustscript:gpio=low");
}

#[test]
fn framework_namespace_imports_compile_correctly() {
    let source =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("programs/framework-api-smoke.rss");
    let compiled = compile_source_file(&source).expect("framework API should compile");
    let imports = compiled
        .program
        .imports
        .iter()
        .map(|import| import.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    for expected in [
        "gpio::configure",
        "gpio::digital_write",
        "gpio::digital_read",
        "gpio::analog_read",
        "gpio::pwm_write",
        "i2c::open",
        "i2c::close",
        "i2c::transmit",
        "i2c::transmit_register",
        "i2c::receive",
        "i2c::receive_register",
        "mcu::delay_ms",
        "mcu::delay_us",
        "mcu::millis",
        "mcu::micros",
        "mcu::cpu_frequency_mhz",
        "mcu::free_heap",
        "mcu::flash_size",
        "mcu::random",
        "wifi::connect",
        "wifi::disconnect",
        "wifi::is_connected",
        "wifi::rssi",
        "wifi::local_ip",
        "bluetooth::enable",
        "bluetooth::disable",
        "bluetooth::is_enabled",
        "serial::write_line",
        "serial::available",
        "serial::read_bytes",
    ] {
        assert!(imports.contains(expected), "missing host import {expected}");
    }
}

#[test]
fn c_abi_rejects_null_program_pointer() {
    let status = unsafe { rustscript_run_vmbc(std::ptr::null(), 4, None, std::ptr::null_mut(), 0) };
    assert_eq!(status, -1);
}

struct FfiReplTransport {
    board: BoardState,
}

impl ReplTransport for FfiReplTransport {
    fn execute(&mut self, program: &[u8], state: &[u8]) -> std::io::Result<(i32, Vec<u8>)> {
        let mut output = RustScriptBuffer::empty();
        let status = unsafe {
            rustscript_repl_run_vmbc(
                program.as_ptr(),
                program.len(),
                state.as_ptr(),
                state.len(),
                Some(host_callback),
                (&mut self.board as *mut BoardState).cast(),
                100_000,
                &mut output,
            )
        };
        let payload = if output.len == 0 {
            Vec::new()
        } else {
            unsafe { slice::from_raw_parts(output.data, output.len) }.to_vec()
        };
        unsafe { rustscript_buffer_free(output) };
        Ok((status, payload))
    }
}

#[test]
fn stateful_repl_runs_line_by_line_through_embedded_ffi() {
    let mut session = SerialReplSession::new();
    let mut transport = FfiReplTransport {
        board: BoardState::default(),
    };

    assert_eq!(
        session.eval("let mut x = 40;", &mut transport).unwrap(),
        None
    );
    assert_eq!(
        session
            .eval(
                "fn gpio_digital_read(pin: int) -> bool; gpio_digital_read(8)",
                &mut transport,
            )
            .unwrap(),
        Some(ReplValue::Bool(false))
    );
    assert_eq!(session.eval("x = x + 2;", &mut transport).unwrap(), None);
    assert_eq!(
        session.eval("x", &mut transport).unwrap(),
        Some(ReplValue::Int(42))
    );
}

#[test]
fn stateful_repl_dispatches_host_calls_through_embedded_ffi() {
    let mut session = SerialReplSession::new();
    let mut transport = FfiReplTransport {
        board: BoardState {
            high: true,
            ..BoardState::default()
        },
    };

    let result = session
        .eval(
            "fn gpio_digital_read(pin: int) -> bool; gpio_digital_read(8)",
            &mut transport,
        )
        .unwrap();
    assert_eq!(result, Some(ReplValue::Bool(true)));
    assert_eq!(transport.board.pin, 8);
    assert_eq!(transport.board.gpio_reads, 1);
}
