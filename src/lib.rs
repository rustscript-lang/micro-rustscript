#![cfg_attr(not(feature = "host"), no_std)]

extern crate alloc;

#[cfg(all(feature = "rp2040", target_os = "none"))]
mod allocator;
#[cfg(feature = "rp2040")]
mod ffi;
#[cfg(feature = "host")]
mod host;

#[cfg(feature = "rp2040")]
pub use ffi::{
    RustScriptHostCallback, RustScriptValue, RustScriptValueError, RustScriptValueTag,
    rustscript_run_vmbc,
};
#[cfg(feature = "host")]
pub use host::*;
