//! Rust binding to the [zstd library][zstd].
//!
//! This crate provides:
//!
//! * An [encoder](struct.Encoder.html) to compress data using zstd and send the output to another write.
//! * A [decoder](struct.Decoder.html) to read input data from a `Read` and decompress it.
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
extern crate libc;

mod ll;
mod encoder;
mod decoder;

pub use encoder::Encoder;
pub use decoder::Decoder;

use std::io;

/// Trains a dictionary to improve compression of small files.
///
/// The dictionary must be present during decompression,
/// but can be shared accross multiple "similar" files.
pub fn train_dict(sample_data: &[u8],
                  sample_sizes: &[usize],
                  max_size: usize)
                  -> io::Result<Vec<u8>> {
    // Complain if the lengths don't add up to the entire data.
    if sample_sizes.iter().fold(0, |a, b| a + b) != sample_data.len() {
        return Err(io::Error::new(io::ErrorKind::Other,
                                  "sample sizes don't add up".to_string()));
    }

    let mut result = Vec::with_capacity(max_size);
    unsafe {
        let code = ll::ZDICT_trainFromBuffer(result.as_mut_ptr(),
                                             result.capacity(),
                                             sample_data.as_ptr(),
                                             sample_sizes.as_ptr(),
                                             sample_sizes.len());
        let written = try!(ll::parse_code(code));
        result.set_len(written);
    }
    Ok(result)
}

#[test]
fn test_cycle() {
    use std::io::{Read, Write};
    let text = "This is a sample text. It is not meant to be interesting or anything. Just text, \
                nothing more. Don't expect too much from it.";

    let mut buffer: Vec<u8> = Vec::new();

    let mut encoder = Encoder::new(buffer, 1).unwrap();
    encoder.write_all(text.as_bytes()).unwrap();
    let buffer = encoder.finish().unwrap();

    let mut decoder = Decoder::new(&buffer[..]).unwrap();
    let mut result = String::new();
    decoder.read_to_string(&mut result).unwrap();

    assert_eq!(text, &result);
}
