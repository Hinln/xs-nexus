#![deny(unsafe_code)]
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(windows)]
#[allow(unsafe_code)]
mod platform;

#[cfg(windows)]
pub use platform::{ensure_private_directory, read_private, remove_private, write_private_atomic};
