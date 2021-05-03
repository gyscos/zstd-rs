#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![no_std]
//! Low-level bindings to the [zstd] library.
//!
//! [zstd]: https://facebook.github.io/zstd/

#[cfg(feature = "std")]
extern crate std;

// If running bindgen, we'll end up with the correct bindings anyway.
#[cfg(feature = "bindgen")]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

// The bindings used depend on a few feature flags.

// No-std (libc-based)
#[cfg(all(
    not(feature = "std"),
    not(feature = "experimental"),
    not(feature = "bindgen")
))]
include!("bindings.rs");

#[cfg(all(
    not(feature = "std"),
    feature = "experimental",
    not(feature = "bindgen")
))]
include!("bindings_experimental.rs");

// Std-based (no libc)
#[cfg(all(
    feature = "std",
    not(feature = "experimental"),
    not(feature = "bindgen")
))]
include!("bindings_std.rs");

#[cfg(all(
    feature = "std",
    feature = "experimental",
    not(feature = "bindgen")
))]
include!("bindings_std_experimental.rs");
