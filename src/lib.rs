//! Rust binding to the [zstd library][zstd].
//!
//! This crate provides:
//!
//! * An [encoder](stream/struct.Encoder.html) to compress data using zstd
//!   and send the output to another write.
//! * A [decoder](stream/struct.Decoder.html) to read input data from a `Read`
//!   and decompress it.
//! * Convenient functions for common tasks.
//!
//! # Example
//!
//! ```no_run
//! extern crate zstd;
//!
//! use std::io;
//!
//! fn main() {
//! 	// Uncompress input and print the result.
//! 	zstd::stream::copy_decode(io::stdin(), io::stdout()).unwrap();
//! }
//! ```
//!
//! [zstd]: https://github.com/facebook/zstd
#![deny(missing_docs)]

#[cfg(test)]
extern crate partial_io;

extern crate zstd_safe;

#[cfg(feature = "tokio")]
#[macro_use]
extern crate tokio_io;
#[cfg(feature = "tokio")]
extern crate futures;
#[cfg(all(test, feature = "tokio"))]
extern crate quickcheck;

pub mod block;
pub mod dict;
pub mod stream;

use std::io;

/// Default compression level.
pub use zstd_safe::CLEVEL_DEFAULT as DEFAULT_COMPRESSION_LEVEL;

#[doc(no_inline)]
pub use stream::{decode_all, encode_all, Decoder, Encoder};

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
