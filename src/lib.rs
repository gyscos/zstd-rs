//! Rust binding to the [zstd library][zstd].
//!
//! This crate provides:
//!
//! * `Encoder` to compress data using zstd and send the output to another write.
//! * `Decoder` to read input data from a `Read` and decompress it.
//!
//! [zstd]: https://github.com/Cyan4973/zstd
extern crate libc;

mod ll;
mod encoder;
mod decoder;

pub use encoder::Encoder;
pub use decoder::Decoder;

#[test]
fn test_cycle() {
    use std::io::{Read,Write};
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
