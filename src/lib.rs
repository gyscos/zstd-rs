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
//! ```rust
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
//! [zstd]: https://github.com/Cyan4973/zstd
#![deny(missing_docs)]
extern crate libc;

mod ll;
mod stream;

pub mod block;
pub mod dict;

pub use stream::encoder::{Encoder, AutoFinishEncoder};
pub use stream::decoder::Decoder;

use std::io;


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


#[test]
fn test_cycle() {
    let text = "This is a sample text. It is not meant to be interesting or \
                anything. Just text, nothing more. Don't expect too much \
                from it.";

    let compressed = encode_all(text.as_bytes(), 1).unwrap();

    let decompressed = String::from_utf8(decode_all(&compressed).unwrap())
                           .unwrap();

    assert_eq!(text, &decompressed);
}
