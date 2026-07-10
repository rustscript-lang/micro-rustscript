use alloc::vec::Vec;
use core::ffi::c_void;
use core::{ptr, slice, str};

use vm::embedded::{HostError, Value, Vm, VmError, decode_program};

pub const RUSTSCRIPT_STATUS_OK: i32 = 0;
pub const RUSTSCRIPT_STATUS_INVALID_ARGUMENT: i32 = -1;
pub const RUSTSCRIPT_STATUS_INVALID_VMBC: i32 = -2;
pub const RUSTSCRIPT_STATUS_HOST_ERROR: i32 = -3;
pub const RUSTSCRIPT_STATUS_OUT_OF_FUEL: i32 = -4;
pub const RUSTSCRIPT_STATUS_VM_ERROR: i32 = -5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum RustScriptValueTag {
    Null = 0,
    Int = 1,
    Float = 2,
    Bool = 3,
    String = 4,
    Bytes = 5,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RustScriptValueError {
    InvalidTag(u8),
    InvalidBool(u8),
    NullData,
    InvalidUtf8,
    UnsupportedType,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct RustScriptValue {
    pub tag: u8,
    pub boolean: u8,
    pub reserved: [u8; 6],
    pub integer: i64,
    pub float: f64,
    pub data: *const u8,
    pub len: usize,
}

impl RustScriptValue {
    pub const fn null() -> Self {
        Self {
            tag: RustScriptValueTag::Null as u8,
            boolean: 0,
            reserved: [0; 6],
            integer: 0,
            float: 0.0,
            data: ptr::null(),
            len: 0,
        }
    }

    pub fn from_embedded(value: &Value) -> Result<Self, RustScriptValueError> {
        let mut output = Self::null();
        match value {
            Value::Null => {}
            Value::Int(value) => {
                output.tag = RustScriptValueTag::Int as u8;
                output.integer = *value;
            }
            Value::Float(value) => {
                output.tag = RustScriptValueTag::Float as u8;
                output.float = *value;
            }
            Value::Bool(value) => {
                output.tag = RustScriptValueTag::Bool as u8;
                output.boolean = u8::from(*value);
            }
            Value::String(value) => {
                output.tag = RustScriptValueTag::String as u8;
                output.data = value.as_ptr();
                output.len = value.len();
            }
            Value::Bytes(value) => {
                output.tag = RustScriptValueTag::Bytes as u8;
                output.data = value.as_ptr();
                output.len = value.len();
            }
            Value::Array(_) | Value::Map(_) => return Err(RustScriptValueError::UnsupportedType),
        }
        Ok(output)
    }

    /// Convert a C ABI value into an owned embedded value.
    ///
    /// # Safety
    ///
    /// For string and byte tags, `data` must point to `len` readable bytes for
    /// the duration of this call. A zero-length value may use a null pointer.
    pub unsafe fn to_embedded(self) -> Result<Value, RustScriptValueError> {
        match self.tag {
            value if value == RustScriptValueTag::Null as u8 => Ok(Value::Null),
            value if value == RustScriptValueTag::Int as u8 => Ok(Value::Int(self.integer)),
            value if value == RustScriptValueTag::Float as u8 => Ok(Value::Float(self.float)),
            value if value == RustScriptValueTag::Bool as u8 => match self.boolean {
                0 => Ok(Value::Bool(false)),
                1 => Ok(Value::Bool(true)),
                value => Err(RustScriptValueError::InvalidBool(value)),
            },
            value if value == RustScriptValueTag::String as u8 => {
                let bytes = unsafe { borrowed_bytes(self.data, self.len)? };
                let text = str::from_utf8(bytes).map_err(|_| RustScriptValueError::InvalidUtf8)?;
                Ok(Value::string(text))
            }
            value if value == RustScriptValueTag::Bytes as u8 => {
                let bytes = unsafe { borrowed_bytes(self.data, self.len)? };
                Ok(Value::bytes(bytes))
            }
            value => Err(RustScriptValueError::InvalidTag(value)),
        }
    }
}

pub type RustScriptHostCallback = unsafe extern "C" fn(
    context: *mut c_void,
    name: *const u8,
    name_len: usize,
    args: *const RustScriptValue,
    arg_count: usize,
    result: *mut RustScriptValue,
) -> i32;

struct CallbackContext {
    callback: Option<RustScriptHostCallback>,
    user_context: *mut c_void,
}

fn dispatch_host(
    context: &mut CallbackContext,
    name: &str,
    args: &[Value],
) -> Result<Option<Value>, HostError> {
    let callback = context
        .callback
        .ok_or_else(|| HostError::new("host callback is missing"))?;
    let mut ffi_args = Vec::new();
    ffi_args
        .try_reserve_exact(args.len())
        .map_err(|_| HostError::new("host argument allocation failed"))?;
    for value in args {
        ffi_args.push(
            RustScriptValue::from_embedded(value)
                .map_err(|_| HostError::new("unsupported host argument"))?,
        );
    }

    let mut result = RustScriptValue::null();
    let status = unsafe {
        callback(
            context.user_context,
            name.as_ptr(),
            name.len(),
            ffi_args.as_ptr(),
            ffi_args.len(),
            &mut result,
        )
    };
    match status {
        0 => Ok(None),
        1 => unsafe { result.to_embedded() }
            .map(Some)
            .map_err(|_| HostError::new("invalid host return value")),
        _ => Err(HostError::new("host callback failed")),
    }
}

/// Decode and execute a VMBC program through the C host callback.
///
/// # Safety
///
/// `program` must point to `program_len` readable bytes. `user_context` must
/// satisfy the callback's own contract and remain valid until this call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rustscript_run_vmbc(
    program: *const u8,
    program_len: usize,
    callback: Option<RustScriptHostCallback>,
    user_context: *mut c_void,
    fuel: u64,
) -> i32 {
    let bytes = match unsafe { borrowed_bytes(program, program_len) } {
        Ok(bytes) if !bytes.is_empty() => bytes,
        _ => return RUSTSCRIPT_STATUS_INVALID_ARGUMENT,
    };
    let program = match decode_program(bytes) {
        Ok(program) => program,
        Err(_) => return RUSTSCRIPT_STATUS_INVALID_VMBC,
    };
    let context = CallbackContext {
        callback,
        user_context,
    };
    let mut vm = Vm::with_host_dispatcher(program, context, dispatch_host);
    if fuel != 0 {
        vm.set_fuel(fuel);
    }
    match vm.run() {
        Ok(_) => RUSTSCRIPT_STATUS_OK,
        Err(VmError::OutOfFuel { .. }) => RUSTSCRIPT_STATUS_OUT_OF_FUEL,
        Err(VmError::HostError(_) | VmError::HostCallsUnavailable(_)) => {
            RUSTSCRIPT_STATUS_HOST_ERROR
        }
        Err(_) => RUSTSCRIPT_STATUS_VM_ERROR,
    }
}

unsafe fn borrowed_bytes<'a>(
    data: *const u8,
    len: usize,
) -> Result<&'a [u8], RustScriptValueError> {
    if len == 0 {
        return Ok(&[]);
    }
    if data.is_null() {
        return Err(RustScriptValueError::NullData);
    }
    Ok(unsafe { slice::from_raw_parts(data, len) })
}
