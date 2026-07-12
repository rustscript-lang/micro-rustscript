#![cfg_attr(not(feature = "host"), no_std)]

extern crate alloc;

#[cfg(all(
    any(feature = "arduino", feature = "esp32c3", feature = "esp32s31"),
    not(feature = "host")
))]
mod allocator;
#[cfg(any(feature = "arduino", feature = "esp32c3", feature = "esp32s31"))]
mod ffi;
#[cfg(feature = "host")]
mod host;
mod repl_wire;

#[cfg(any(feature = "arduino", feature = "esp32c3", feature = "esp32s31"))]
pub use ffi::{
    RustScriptHostCallback, RustScriptValue, RustScriptValueError, RustScriptValueTag,
    rustscript_run_vmbc,
};
#[cfg(feature = "host")]
pub use host::*;
pub use repl_wire::{
    ReplResponse, ReplValue, ReplWireError, decode_repl_response, decode_repl_state,
    encode_repl_response, encode_repl_state,
};
