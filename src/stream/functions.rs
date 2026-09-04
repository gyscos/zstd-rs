use std::io;
use std::io::BufRead;

use super::{Decoder, Encoder};

/// How much `decode_all` will reserve up front, at most.
///
/// The size comes out of the frame header, which is whatever the data claims
/// it is - so it is capped, or a few bytes of input could ask us for an
/// arbitrary amount of memory. Past this the output just grows as it goes,
/// which is what the whole function used to do; the reserve is only worth
/// anything for the first few megabytes anyway.
const MAX_RESERVED: usize = 32 * 1024 * 1024;

/// Reads the decompressed size out of the start of a frame, if it says.
///
/// Returns `None` if `data` does not begin with a zstd frame header, or if the
/// frame does not record its content size - which is normal for anything
/// produced by a streaming compressor that was not told the size in advance.
///
/// This is the size to reserve when decompressing into your own buffer:
///
/// ```
/// # fn main() -> std::io::Result<()> {
/// let frame = zstd::encode_all(&b"some data"[..], 3)?;
///
/// let mut output = match zstd::decompressed_size(&frame) {
///     Some(size) => Vec::with_capacity(size as usize),
///     None => Vec::new(),
/// };
/// zstd::stream::copy_decode(&frame[..], &mut output)?;
/// # assert_eq!(output, b"some data");
/// # Ok(())
/// # }
/// ```
pub fn decompressed_size(data: &[u8]) -> Option<u64> {
    zstd_safe::get_frame_content_size(data).ok().flatten()
}

/// Decompress from the given source as if using a `Decoder`.
///
/// The input data must be in the zstd frame format.
///
/// When the frame records its decompressed size, the returned `Vec` is sized
/// for it up front. That is worth about a factor of two over letting it grow,
/// so prefer this to driving a `Decoder` into a `Vec::new()` yourself. If you
/// are decompressing into a buffer you own, [`decompressed_size()`] gives you
/// the same number.
pub fn decode_all<R: io::Read>(source: R) -> io::Result<Vec<u8>> {
    // Peek at the frame header - `fill_buf` does not consume it - so the
    // output can be sized before we start.
    let mut source =
        io::BufReader::with_capacity(zstd_safe::DCtx::in_size(), source);
    let reserve = decompressed_size(source.fill_buf()?)
        .unwrap_or(0)
        .min(MAX_RESERVED as u64) as usize;

    let mut result = Vec::new();
    // A failed reservation is not fatal: decoding still works, just without
    // the head start. This also keeps a huge claimed size from aborting.
    let _ = result.try_reserve(reserve);

    let mut decoder = Decoder::with_buffer(source)?;
    io::copy(&mut decoder, &mut result)?;
    Ok(result)
}

/// Decompress from the given source as if using a `Decoder`.
///
/// Decompressed data will be appended to `destination`.
pub fn copy_decode<R, W>(source: R, mut destination: W) -> io::Result<()>
where
    R: io::Read,
    W: io::Write,
{
    let mut decoder = Decoder::new(source)?;
    io::copy(&mut decoder, &mut destination)?;
    Ok(())
}

/// Compress all data from the given source as if using an `Encoder`.
///
/// Result will be in the zstd frame format.
///
/// A level of `0` uses zstd's default (currently `3`).
///
/// Because the source is read as a stream, its length is not known in advance,
/// so the frame this produces does not record its decompressed size. Whoever
/// decompresses it cannot size their buffer up front, which costs them about a
/// factor of two - see [`decompressed_size()`]. If you are compressing a slice
/// you already hold, [`crate::bulk::compress()`] records the size, and is
/// faster here too.
pub fn encode_all<R: io::Read>(source: R, level: i32) -> io::Result<Vec<u8>> {
    let mut result = Vec::<u8>::new();
    copy_encode(source, &mut result, level)?;
    Ok(result)
}

/// Compress all data from the given source as if using an `Encoder`.
///
/// Compressed data will be appended to `destination`.
///
/// A level of `0` uses zstd's default (currently `3`).
pub fn copy_encode<R, W>(
    mut source: R,
    destination: W,
    level: i32,
) -> io::Result<()>
where
    R: io::Read,
    W: io::Write,
{
    let mut encoder = Encoder::new(destination, level)?;
    io::copy(&mut source, &mut encoder)?;
    encoder.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {}
