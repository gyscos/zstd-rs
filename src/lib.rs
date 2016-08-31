//! Rust binding to the [zstd library][zstd].
//!
//! This crate provides:
//!
//! * An [encoder](struct.Encoder.html) to compress data using zstd
//!   and send the output to another write.
//! * A [decoder](struct.Decoder.html) to read input data from a `Read`
//!   and decompress it.
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
//! 	let mut decoder = zstd::Decoder::new(io::stdin()).unwrap();
//! 	io::copy(&mut decoder, &mut io::stdout()).unwrap();
//! }
//! ```
//!
//! [zstd]: https://github.com/facebook/zstd
#![deny(missing_docs)]
extern crate libc;

mod ll;
mod stream;

pub mod block;
pub mod dict;

pub use stream::encoder::{AutoFinishEncoder, Encoder};
pub use stream::decoder::Decoder;

use std::io;
use std::ffi::CStr;


/// Decompress the given data as if using a `Decoder`.
///
/// The input data must be in the zstd frame format.
pub fn decode_all(data: &[u8]) -> io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut decoder = try!(Decoder::new(data));
    try!(io::copy(&mut decoder, &mut result));
    Ok(result)
}

/// Compress all the given data as if using an `Encoder`.
///
/// Result will be in the zstd frame format.
pub fn encode_all(data: &[u8], level: i32) -> io::Result<Vec<u8>> {
    let result = Vec::<u8>::new();
    let mut encoder = try!(Encoder::new(result, level));
    let mut input = data;
    try!(io::copy(&mut input, &mut encoder));
    encoder.finish()
}

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

#[cfg(test)]
fn test_cycle<I1: ?Sized, O1, I2: ?Sized, O2, F, G>(value: &I1, f: F, g: G)
    where F: FnOnce(&I1) -> O1,
          G: FnOnce(&I2) -> O2,
          I1: PartialEq + std::fmt::Debug,
          O1: std::ops::Deref<Target = I2>,
          O2: std::ops::Deref<Target = I1>
{
    let mid = f(value);
    let end = g(mid.deref());
    assert_eq!(value, end.deref());
}

#[cfg(test)]
fn test_cycle_unwrap<I1: ?Sized, O1, I2: ?Sized, O2, E1, E2, F, G>(value: &I1,
                                                                   f: F, g: G)
    where F: FnOnce(&I1) -> Result<O1, E1>,
          G: FnOnce(&I2) -> Result<O2, E2>,
          I1: PartialEq + std::fmt::Debug,
          O1: std::ops::Deref<Target = I2>,
          O2: std::ops::Deref<Target = I1>,
          E1: std::fmt::Debug,
          E2: std::fmt::Debug,
{
    test_cycle(value, |data| f(data).unwrap(), |data| g(data).unwrap());
}

#[cfg(test)]
mod tests {
    use {decode_all, encode_all};

    // Check that compressing+decompressing some data gives back the original
    fn test_full_cycle(input: &[u8], level: i32) {
        ::test_cycle_unwrap(input, |data| encode_all(data, level), decode_all);
    }


    #[test]
    fn test_ll_source() {
        // Where could I find some long text?...
        let data = include_bytes!("ll.rs");
        // Test a few compression levels.
        // TODO: check them all?
        for level in 1..5 {
            test_full_cycle(data, level);
        }
    }
}
