//! Rust binding to the [zstd library][zstd].
//!
//! This crate provides:
//!
//! * An [encoder](stream/write/struct.Encoder.html) to compress data using zstd
//!   and send the output to another write.
//! * A [decoder](stream/read/struct.Decoder.html) to read input data from a `Read`
//!   and decompress it.
//! * Convenient functions for common tasks.
//!
//! # Example
//!
//! ```no_run
//! use std::io;
//!
//! // Uncompress input and print the result.
//! zstd::stream::copy_decode(io::stdin(), io::stdout()).unwrap();
//! ```
//!
//! # Cargo features
//!
//! Enabled by default: `legacy`, `arrays`, `zdict_builder`.
//!
//! | Feature | Default | Description |
//! | --- | --- | --- |
//! | `arrays` | yes | Use fixed-size arrays (`[u8; N]`) as output buffers. |
//! | `legacy` | yes | Decode frames written by zstd versions older than 0.8. |
//! | `zdict_builder` | yes | Train new dictionaries. *Using* a dictionary always works. |
//! | `experimental` | | Expose zstd's experimental API, which has no stability guarantees. |
//! | `zstdmt` | | Multi-threaded compression inside the C library. |
//! | `thin` | | Build a smaller C library, at some cost in speed and error reporting. |
//! | `debug` | | Enable zstd's debug logs. |
//! | `no_asm` | | Do not build the x86-64 assembly. |
//! | `fat-lto`, `thin-lto` | | Cross-language LTO. Only works if `clang` builds the C library. |
//! | `bindgen` | | Generate the bindings at build time rather than using the pre-generated ones. |
//! | `pkg-config` | | Link against a system-installed libzstd instead of building the bundled source. |
//! | `vendored` | | Always build the bundled source, even when `pkg-config` is also enabled. |
//! | `cmake` | | Build the C library with zstd's own CMake files instead of the `cc` crate. |
//!
//! See the [readme] for more about the build-related ones.
//!
//! [zstd]: https://github.com/facebook/zstd
//! [readme]: https://github.com/gyscos/zstd-rs#cargo-features
#![deny(missing_docs)]
#![cfg_attr(feature = "doc-cfg", feature(doc_cfg))]

// Re-export the zstd-safe crate.
pub use zstd_safe;

pub mod bulk;
pub mod dict;

#[macro_use]
pub mod stream;

use std::io;

/// Default compression level.
pub use zstd_safe::CLEVEL_DEFAULT as DEFAULT_COMPRESSION_LEVEL;

/// The accepted range of compression levels.
pub fn compression_level_range(
) -> std::ops::RangeInclusive<zstd_safe::CompressionLevel> {
    zstd_safe::min_c_level()..=zstd_safe::max_c_level()
}

#[doc(no_inline)]
pub use crate::stream::{decode_all, encode_all, Decoder, Encoder};

/// Returns the error message as io::Error based on error_code.
fn map_error_code(code: usize) -> io::Error {
    let msg = zstd_safe::get_error_name(code);
    io::Error::new(io::ErrorKind::Other, msg.to_string())
}

// Some helper functions to write full-cycle tests.

#[cfg(test)]
fn test_cycle<F, G>(data: &[u8], f: F, g: G)
where
    F: Fn(&[u8]) -> Vec<u8>,
    G: Fn(&[u8]) -> Vec<u8>,
{
    let mid = f(data);
    let end = g(&mid);
    assert_eq!(data, &end[..]);
}

#[cfg(test)]
fn test_cycle_unwrap<F, G>(data: &[u8], f: F, g: G)
where
    F: Fn(&[u8]) -> io::Result<Vec<u8>>,
    G: Fn(&[u8]) -> io::Result<Vec<u8>>,
{
    test_cycle(data, |data| f(data).unwrap(), |data| g(data).unwrap())
}

#[test]
fn default_compression_level_in_range() {
    assert!(compression_level_range().contains(&DEFAULT_COMPRESSION_LEVEL));
}
