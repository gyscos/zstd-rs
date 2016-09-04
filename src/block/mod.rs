//! Compress and decompress individual blocks.
//!
//! These methods process all the input data at once.
//! It is therefore best used with relatively small blocks
//! (like small network packets).

mod compressor;
mod decompressor;

pub use self::compressor::Compressor;
pub use self::decompressor::Decompressor;

use std::io;

/// Compresses a single block of data to the given destination buffer.
///
/// Returns the number of bytes written, or an error if something happened
/// (for instance if the destination buffer was too small).
pub fn compress_to_buffer(source: &[u8], destination: &mut [u8], level: i32)
                          -> io::Result<usize> {

    Compressor::new().compress_to_buffer(source, destination, level)
}

/// Compresses a block of data and returns the compressed result.
pub fn compress(data: &[u8], level: i32) -> io::Result<Vec<u8>> {
    Compressor::new().compress(data, level)
}

/// Deompress a single block of data to the given destination buffer.
///
/// Returns the number of bytes written, or an error if something happened
/// (for instance if the destination buffer was too small).
pub fn decompress_to_buffer(source: &[u8], destination: &mut [u8])
                            -> io::Result<usize> {
    Decompressor::new().decompress_to_buffer(source, destination)
}

/// Decompresses a block of data and returns the decompressed result.
///
/// The decompressed data should be less than `capacity` bytes,
/// or an error will be returned.
pub fn decompress(data: &[u8], capacity: usize) -> io::Result<Vec<u8>> {
    Decompressor::new().decompress(data, capacity)
}

#[cfg(test)]
mod tests {
    use super::{compress, decompress};

    #[test]
    fn test_direct() {
        // hipsum.co
        let text = "Pork belly art party wolf XOXO, neutra scenester ugh \
                    thundercats tattooed squid skateboard beard readymade \
                    kogi. VHS cardigan schlitz, meditation chartreuse kogi \
                    tilde church-key. Actually direct trade hammock, \
                    aesthetic VHS semiotics organic narwhal lo-fi heirloom \
                    flexitarian master cleanse polaroid man bun. Flannel \
                    helvetica mustache, bicycle rights small batch slow-carb \
                    neutra tilde williamsburg meh poutine humblebrag. Four \
                    dollar toast butcher actually franzen, gastropub \
                    mustache tofu cardigan. 90's fingerstache forage \
                    brooklyn meditation single-origin coffee tofu actually, \
                    ramps pabst farm-to-table art party kombucha artisan \
                    fanny pack. Flannel salvia ennui viral leggings selfies.";

        ::test_cycle_unwrap(text.as_bytes(),
                            |data| compress(data, 1),
                            |data| decompress(data, text.len()));
    }
}
