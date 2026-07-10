#![cfg(feature = "esp32")]

use std::ffi::c_void;
use std::slice;

use pd_vm_nostd::Value as NoStdValue;
use rustscript_embedded::{
    RustScriptHostCallback, RustScriptValue, RustScriptValueTag, rustscript_run_vmbc,
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
        b"__esp32_gpio_configure"
            if args.len() == 2
                && args[0].tag == RustScriptValueTag::Int as u8
                && args[1].tag == RustScriptValueTag::Int as u8 =>
        {
            state.pin = args[0].integer;
            state.mode = args[1].integer;
            unsafe { set_bool_result(result, true) }
        }
        b"__esp32_gpio_write"
            if args.len() == 2
                && args[0].tag == RustScriptValueTag::Int as u8
                && args[1].tag == RustScriptValueTag::Bool as u8 =>
        {
            state.pin = args[0].integer;
            state.high = args[1].boolean != 0;
            state.gpio_writes += 1;
            unsafe { set_bool_result(result, true) }
        }
        b"__esp32_gpio_read" if args.len() == 1 && args[0].tag == RustScriptValueTag::Int as u8 => {
            state.pin = args[0].integer;
            state.gpio_reads += 1;
            unsafe { set_bool_result(result, state.high) }
        }
        b"__esp32_mcu_delay_ms"
            if args.len() == 1 && args[0].tag == RustScriptValueTag::Int as u8 =>
        {
            state.delayed_ms += args[0].integer;
            0
        }
        b"__esp32_serial_write"
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
            fn __esp32_gpio_write(pin: int, high: bool) -> bool;
            let ok: bool = __esp32_gpio_write(8, true);
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
fn framework_modules_compile_to_private_board_imports() {
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
        "__esp32_gpio_configure",
        "__esp32_gpio_write",
        "__esp32_gpio_read",
        "__esp32_gpio_analog_read",
        "__esp32_gpio_pwm",
        "__esp32_i2c_begin",
        "__esp32_i2c_end",
        "__esp32_i2c_write",
        "__esp32_i2c_write_register",
        "__esp32_i2c_read",
        "__esp32_i2c_read_register",
        "__esp32_mcu_delay_ms",
        "__esp32_mcu_free_heap",
        "__esp32_serial_write",
        "__esp32_serial_read",
    ] {
        assert!(imports.contains(expected), "missing host import {expected}");
    }
}

#[test]
fn c_abi_rejects_null_program_pointer() {
    let status = unsafe { rustscript_run_vmbc(std::ptr::null(), 4, None, std::ptr::null_mut(), 0) };
    assert_eq!(status, -1);
}
