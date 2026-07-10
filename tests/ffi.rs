#![cfg(feature = "rp2040")]

use std::ffi::c_void;
use std::slice;

use rustscript_embedded::{
    RustScriptHostCallback, RustScriptValue, RustScriptValueTag, rustscript_run_vmbc,
};
use vm::{compile_source_for_repl, encode_program};

#[derive(Default, Debug, PartialEq, Eq)]
struct BoardState {
    pin: i64,
    high: bool,
}

unsafe extern "C" fn host_callback(
    context: *mut c_void,
    name: *const u8,
    name_len: usize,
    args: *const RustScriptValue,
    arg_count: usize,
    _result: *mut RustScriptValue,
) -> i32 {
    let state = unsafe { &mut *context.cast::<BoardState>() };
    let name = unsafe { slice::from_raw_parts(name, name_len) };
    let args = unsafe { slice::from_raw_parts(args, arg_count) };
    if name != b"gpio_set" || args.len() != 2 {
        return -1;
    }
    if args[0].tag != RustScriptValueTag::Int as u8 || args[1].tag != RustScriptValueTag::Bool as u8
    {
        return -1;
    }
    state.pin = args[0].integer;
    state.high = args[1].boolean != 0;
    0
}

fn compile_vmbc(source: &str) -> Vec<u8> {
    let compiled = compile_source_for_repl(source).expect("source should compile");
    encode_program(&compiled.program.with_local_count(compiled.locals))
        .expect("program should encode")
}

#[test]
fn scalar_ffi_values_round_trip() {
    let values = [
        vm::embedded::Value::Null,
        vm::embedded::Value::Int(42),
        vm::embedded::Value::Float(2.5),
        vm::embedded::Value::Bool(true),
        vm::embedded::Value::string("pico"),
        vm::embedded::Value::bytes([1, 2, 3]),
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
            fn gpio_set(pin: int, high: bool);
            gpio_set(25, true);
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
    assert_eq!(
        state,
        BoardState {
            pin: 25,
            high: true,
        }
    );
}

#[test]
fn c_abi_rejects_null_program_pointer() {
    let status = unsafe { rustscript_run_vmbc(std::ptr::null(), 4, None, std::ptr::null_mut(), 0) };
    assert_eq!(status, -1);
}
