#![cfg_attr(not(feature = "host"), no_std)]

extern crate alloc;

#[cfg(all(any(feature = "arduino", feature = "esp32"), not(feature = "host")))]
mod allocator;
#[cfg(any(feature = "arduino", feature = "esp32"))]
mod ffi;
#[cfg(feature = "host")]
mod host;

#[cfg(any(feature = "arduino", feature = "esp32"))]
pub use ffi::{
    RustScriptHostCallback, RustScriptValue, RustScriptValueError, RustScriptValueTag,
    rustscript_run_vmbc,
};
#[cfg(feature = "host")]
pub use host::*;
