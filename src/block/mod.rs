//! Methods to compress and decompress individual blocks.
//!
//! These methods process all the input data at once.
//! It is therefore best used with relatively small blocks
//! (like small network packets).

mod compressor;
mod decompressor;

pub use self::compressor::Compressor;
pub use self::decompressor::Decompressor;

use std::io;

/// Compress a single block of data to the given destination buffer.
///
/// Returns the number of bytes written, or an error if something happened
/// (for instance if the destination buffer was too small).
pub fn compress_to_buffer(destination: &mut [u8], source: &[u8], level: i32)
                          -> io::Result<usize> {

    Compressor::new().compress_to_buffer(destination, source, level)
}

/// Compress a block of data, and return the compressed result in a `Vec<u8>`.
pub fn compress(data: &[u8], level: i32) -> io::Result<Vec<u8>> {
    Compressor::new().compress(data, level)
}

/// Deompress a single block of data to the given destination buffer.
///
/// Returns the number of bytes written, or an error if something happened
/// (for instance if the destination buffer was too small).
pub fn decompress_to_buffer(destination: &mut [u8], source: &[u8])
                            -> io::Result<usize> {
    Decompressor::new().decompress_to_buffer(destination, source)
}

/// Decompress a block of data, and return the decompressed result in a `Vec<u8>`.
///
/// The decompressed data should be less than `capacity` bytes,
/// or an error will be returned.
pub fn decompress(data: &[u8], capacity: usize) -> io::Result<Vec<u8>> {
    Decompressor::new().decompress(data, capacity)
}

#[test]
fn test_direct() {
    // hipsum.co
    let text = "Pork belly art party wolf XOXO, neutra scenester ugh \
                thundercats tattooed squid skateboard beard readymade kogi. \
                VHS cardigan schlitz, meditation chartreuse kogi tilde \
                church-key. Actually direct trade hammock, aesthetic VHS \
                semiotics organic narwhal lo-fi heirloom flexitarian master \
                cleanse polaroid man bun. Flannel helvetica mustache, \
                bicycle rights small batch slow-carb neutra tilde \
                williamsburg meh poutine humblebrag. Four dollar toast \
                butcher actually franzen, gastropub mustache tofu cardigan. \
                90's fingerstache forage brooklyn meditation single-origin \
                coffee tofu actually, ramps pabst farm-to-table art party \
                kombucha artisan fanny pack. Flannel salvia ennui viral \
                leggings selfies.";

    let compressed = compress(text.as_bytes(), 1).unwrap();

    let uncompressed = decompress(&compressed, text.len()).unwrap();

    assert_eq!(text.as_bytes(), &uncompressed[..]);
}
