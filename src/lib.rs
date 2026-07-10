#![cfg_attr(not(feature = "host"), no_std)]

extern crate alloc;

#[cfg(all(feature = "esp32", target_os = "none"))]
mod allocator;
#[cfg(feature = "esp32")]
mod ffi;
#[cfg(feature = "host")]
mod host;

#[cfg(feature = "esp32")]
pub use ffi::{
    RustScriptHostCallback, RustScriptValue, RustScriptValueError, RustScriptValueTag,
    rustscript_run_vmbc,
};
#[cfg(feature = "host")]
pub use host::*;
