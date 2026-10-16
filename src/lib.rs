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

/// How large a buffer a compressed block might need, at worst.
///
/// This is what to reserve before [`bulk::compress_to_buffer()`], which writes
/// into a destination you provide rather than allocating one.
///
/// Re-exported from `zstd_safe`, where it lives alongside the rest of the
/// direct bindings.
pub use zstd_safe::compress_bound;

#[doc(no_inline)]
pub use crate::stream::{
    compress_file, compress_file_into, compress_sized, compress_sized_into,
    decode_all, decompressed_size, encode_all, Decoder, Encoder,
};

/// An error reported by the zstd library.
///
/// When zstd itself fails, the `io::Error` this crate returns carries one of
/// these, so you can tell what went wrong without matching on the message:
///
/// ```
/// let mut small = [0u8; 4];
/// let err = zstd::bulk::compress_to_buffer(&[42u8; 1000], &mut small[..], 3)
///     .unwrap_err();
/// let zstd_err = err
///     .get_ref()
///     .and_then(|e| e.downcast_ref::<zstd::Error>())
///     .unwrap();
/// use zstd::zstd_safe::zstd_sys::ZSTD_ErrorCode;
/// assert!(zstd_err.is(ZSTD_ErrorCode::ZSTD_error_dstSize_tooSmall));
/// ```
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Error {
    code: zstd_safe::ErrorCode,
}

impl Error {
    /// Returns whether this is an error of the given kind.
    ///
    /// See [`zstd_safe::is_error_kind()`].
    pub fn is(&self, kind: zstd_safe::zstd_sys::ZSTD_ErrorCode) -> bool {
        zstd_safe::is_error_kind(self.code, kind)
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Error")
            .field(&zstd_safe::get_error_name(self.code))
            .finish()
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(zstd_safe::get_error_name(self.code))
    }
}

impl std::error::Error for Error {}

/// Wraps a zstd error code in an `io::Error`, as an [`Error`].
fn map_error_code(code: usize) -> io::Error {
    io::Error::new(io::ErrorKind::Other, Error { code })
}

/// Returns an empty `Vec` with room for exactly `capacity` bytes.
///
/// Unlike `Vec::with_capacity`, this fails with an `OutOfMemory` error instead
/// of aborting the process when that much memory cannot be allocated.
fn vec_with_capacity(capacity: usize) -> io::Result<Vec<u8>> {
    let mut vec = Vec::new();
    vec.try_reserve_exact(capacity).map_err(|_| {
        io::Error::new(
            io::ErrorKind::OutOfMemory,
            format!("could not allocate a buffer of {} bytes", capacity),
        )
    })?;
    Ok(vec)
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
