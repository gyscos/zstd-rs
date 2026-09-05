//! Compress and decompress data in bulk.
//!
//! These methods process all the input data at once.
//! It is therefore best used with relatively small blocks
//! (like small network packets).

mod compressor;
mod decompressor;

#[cfg(test)]
mod tests;

pub use self::compressor::Compressor;
pub use self::decompressor::Decompressor;

use std::io;

/// Compresses a single block of data to the given destination buffer.
///
/// Returns the number of bytes written, or an error if something happened
/// (for instance if the destination buffer was too small).
///
/// [`crate::compress_bound()`] says how large the destination might need to
/// be. Passing a `Vec` instead of a slice works too, and then it is the
/// `Vec`'s *capacity* that bounds the output - so `reserve` what you need
/// rather than `resize`, since the bytes do not have to be initialized.
///
/// A level of `0` uses zstd's default (currently `3`).
pub fn compress_to_buffer(
    source: &[u8],
    destination: &mut [u8],
    level: i32,
) -> io::Result<usize> {
    Compressor::new(level)?.compress_to_buffer(source, destination)
}

/// Compresses a block of data and returns the compressed result.
///
/// A level of `0` uses zstd's default (currently `3`).
pub fn compress(data: &[u8], level: i32) -> io::Result<Vec<u8>> {
    Compressor::new(level)?.compress(data)
}

/// Decompress a single block of data to the given destination buffer.
///
/// Returns the number of bytes written, or an error if something happened
/// (for instance if the destination buffer was too small).
///
/// [`crate::decompressed_size()`] says how big the result will be, when the
/// frame records it - there is no need to store the size alongside the data
/// yourself. Passing a `Vec` instead of a slice works too, and then it is the
/// `Vec`'s *capacity* that bounds the output, so `reserve` what you need
/// rather than `resize`.
pub fn decompress_to_buffer(
    source: &[u8],
    destination: &mut [u8],
) -> io::Result<usize> {
    Decompressor::new()?.decompress_to_buffer(source, destination)
}

/// Decompresses a block of data and returns the decompressed result.
///
/// The decompressed data should be at most `capacity` bytes, or an error will
/// be returned - so `capacity` is the most you are willing to allocate for
/// this, not a guess that has to be right. When the frame records its size,
/// only that much is allocated however large a `capacity` you allow.
///
/// [`crate::decompressed_size()`] gives you that size up front, if you would
/// rather decide for yourself.
pub fn decompress(data: &[u8], capacity: usize) -> io::Result<Vec<u8>> {
    Decompressor::new()?.decompress(data, capacity)
}
