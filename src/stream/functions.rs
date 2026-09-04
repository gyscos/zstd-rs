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
/// factor of two - see [`decompressed_size()`]. Use [`crate::bulk::compress()`]
/// for a slice you already hold, [`compress_from_file()`] for a file, or
/// [`compress_with_size()`] when you know the length some other way; all three
/// record it.
pub fn encode_all<R: io::Read>(source: R, level: i32) -> io::Result<Vec<u8>> {
    let mut result = Vec::<u8>::new();
    copy_encode(source, &mut result, level)?;
    Ok(result)
}

/// Compress everything from `source`, whose length you already know.
///
/// The length is recorded in the frame header, which is what lets whoever
/// decompresses it size their buffer up front - worth about a factor of two to
/// them. [`encode_all()`] cannot do this, because it has no way to know how
/// much is coming.
///
/// `size` must be exactly how many bytes `source` yields; zstd fails the frame
/// if it turns out to be wrong.
///
/// For data already in a slice, use [`crate::bulk::compress()`], which knows
/// the length without being told. For a file, [`compress_from_file()`].
///
/// ```
/// # fn main() -> std::io::Result<()> {
/// let data = b"the length of this is known up front";
///
/// let frame = zstd::compress_with_size(&data[..], 3, data.len() as u64)?;
///
/// assert_eq!(zstd::decompressed_size(&frame), Some(data.len() as u64));
/// # assert_eq!(zstd::decode_all(&frame[..])?, data);
/// # Ok(())
/// # }
/// ```
pub fn compress_with_size<R: io::Read>(
    mut source: R,
    level: i32,
    size: u64,
) -> io::Result<Vec<u8>> {
    let mut result = Vec::new();
    {
        let mut encoder = Encoder::new(&mut result, level)?;
        encoder.set_pledged_src_size(Some(size))?;
        io::copy(&mut source, &mut encoder)?;
        encoder.finish()?;
    }
    Ok(result)
}

/// Compress the contents of a file.
///
/// The file's length comes from its metadata and is recorded in the frame, so
/// whoever decompresses it can size their buffer up front. The contents are
/// streamed through the compressor rather than read into memory first, so only
/// the compressed result is held.
///
/// ```no_run
/// # fn main() -> std::io::Result<()> {
/// let frame = zstd::compress_from_file("input.txt", 3)?;
/// # Ok(())
/// # }
/// ```
pub fn compress_from_file<P: AsRef<std::path::Path>>(
    path: P,
    level: i32,
) -> io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let size = file.metadata()?.len();

    // Read in chunks the compressor is happy with, rather than leaving
    // `io::copy` to pull from the file 8 KiB at a time.
    let source =
        io::BufReader::with_capacity(zstd_safe::CCtx::in_size(), file);

    compress_with_size(source, level, size)
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
