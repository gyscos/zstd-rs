#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]
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
extern crate libc;

mod ll;
mod encoder;
mod decoder;
pub mod dict;

pub use encoder::Encoder;
pub use decoder::Decoder;

use std::io;


/// Compress a single block of data to the given destination buffer.
///
/// Returns the number of bytes written, or an error if something happened
/// (for instance if the destination buffer was too small).
pub fn compress(destination: &mut [u8], source: &[u8], level: i32) -> io::Result<usize> {
    let code = unsafe {
        ll::ZSTD_compress(destination.as_mut_ptr(),
                          destination.len(),
                          source.as_ptr(),
                          source.len(),
                          level)
    };
    ll::parse_code(code)
}

/// Compress a block of data, and return the compressed result in a `Vec<u8>`.
pub fn compress_to_vec(data: &[u8], level: i32) -> io::Result<Vec<u8>> {
    let buffer_len = unsafe { ll::ZSTD_compressBound(data.len()) };
    let mut buffer = Vec::with_capacity(buffer_len);

    unsafe {
        // Use all capacity. Memory may not be initialized, but we won't read it.
        buffer.set_len(buffer_len);
        let len = try!(compress(&mut buffer[..], data, level));
        buffer.set_len(len);
    }
    Ok(buffer)
}

/// Deompress a single block of data to the given destination buffer.
///
/// Returns the number of bytes written, or an error if something happened
/// (for instance if the destination buffer was too small).
pub fn decompress(destination: &mut [u8], source: &[u8]) -> io::Result<usize> {
    let code = unsafe {
        ll::ZSTD_decompress(destination.as_mut_ptr(),
                            destination.len(),
                            source.as_ptr(),
                            source.len())
    };
    ll::parse_code(code)
}

/// Decompress a block of data, and return the decompressed result in a `Vec<u8>`.
///
/// The decompressed data should be less than `capacity` bytes,
/// or an error will be returned.
pub fn decompress_to_vec(data: &[u8], capacity: usize) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::with_capacity(capacity);
    unsafe {
        buffer.set_len(capacity);
        let len = try!(decompress(&mut buffer[..], data));
        buffer.set_len(len);
    }
    Ok(buffer)
}

#[test]
fn test_cycle() {
    use std::io::{Read, Write};
    let text = "This is a sample text. It is not meant to be interesting or anything. Just text, \
                nothing more. Don't expect too much from it.";

    let buffer: Vec<u8> = Vec::new();

    let mut encoder = Encoder::new(buffer, 1).unwrap();
    encoder.write_all(text.as_bytes()).unwrap();
    let buffer = encoder.finish().unwrap();

    let mut decoder = Decoder::new(&buffer[..]).unwrap();
    let mut result = String::new();
    decoder.read_to_string(&mut result).unwrap();

    assert_eq!(text, &result);
}

#[test]
fn test_direct() {
    let text = "Pork belly art party wolf XOXO, neutra scenester ugh thundercats tattooed squid \
                skateboard beard readymade kogi. VHS cardigan schlitz, meditation chartreuse kogi \
                tilde church-key. Actually direct trade hammock, aesthetic VHS semiotics organic \
                narwhal lo-fi heirloom flexitarian master cleanse polaroid man bun. Flannel \
                helvetica mustache, bicycle rights small batch slow-carb neutra tilde \
                williamsburg meh poutine humblebrag. Four dollar toast butcher actually franzen, \
                gastropub mustache tofu cardigan. 90's fingerstache forage brooklyn meditation \
                single-origin coffee tofu actually, ramps pabst farm-to-table art party kombucha \
                artisan fanny pack. Flannel salvia ennui viral leggings selfies.";

    let compressed = compress_to_vec(text.as_bytes(), 1).unwrap();

    let uncompressed = decompress_to_vec(&compressed, text.len()).unwrap();

    assert_eq!(text.as_bytes(), &uncompressed[..]);
}
