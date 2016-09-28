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
extern crate libc;

mod ll;

pub mod stream;
pub mod block;
pub mod dict;

#[doc(no_inline)]
pub use stream::{Decoder, Encoder, decode_all, encode_all};

use std::io;
use std::ffi::CStr;

/// Parse the result code
///
/// Returns the number of bytes written if the code represents success,
/// or the error message otherwise.
fn parse_code(code: libc::size_t) -> Result<usize, io::Error> {
    unsafe {
        if ll::ZSTD_isError(code) == 0 {
            Ok(code as usize)
        } else {
            let msg = CStr::from_ptr(ll::ZSTD_getErrorName(code));
            let error = io::Error::new(io::ErrorKind::Other,
                                       msg.to_str().unwrap().to_string());
            Err(error)
        }
    }
}

// Some helper functions to write full-cycle tests.

#[cfg(test)]
fn test_cycle<F, G>(data: &[u8], f: F, g: G)
    where F: Fn(&[u8]) -> Vec<u8>,
          G: Fn(&[u8]) -> Vec<u8>
{
    let mid = f(data);
    let end = g(&mid);
    assert_eq!(data, &end[..]);
}

#[cfg(test)]
fn test_cycle_unwrap<F, G>(data: &[u8], f: F, g: G)
    where F: Fn(&[u8]) -> io::Result<Vec<u8>>,
          G: Fn(&[u8]) -> io::Result<Vec<u8>>
{
    test_cycle(data, |data| f(data).unwrap(), |data| g(data).unwrap())
}
