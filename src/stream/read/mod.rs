//! Implement pull-based [`Read`] trait for both compressing and decompressing.
#[cfg(feature = "experimental")]
use std::cmp::min;
#[cfg(feature = "experimental")]
use std::io::{SeekFrom, Seek, Write};
use std::io::{self, BufRead, BufReader, Read};
#[cfg(feature = "experimental")]
use std::mem::size_of;

use crate::dict::{DecoderDictionary, EncoderDictionary};
use crate::stream::{raw, zio};
use zstd_safe;

#[cfg(feature = "experimental")]
use zstd_safe::{frame_header_size, MAGIC_SKIPPABLE_MASK, MAGIC_SKIPPABLE_START, SKIPPABLEHEADERSIZE};
#[cfg(feature = "experimental")]
use super::raw::MagicVariant;

#[cfg(test)]
mod tests;

#[cfg(feature = "experimental")]
const U24_SIZE: usize = size_of::<u16>() + size_of::<u8>();
#[cfg(feature = "experimental")]
const U32_SIZE: usize = size_of::<u32>();

/// A decoder that decompress input data from another `Read`.
///
/// This allows to read a stream of compressed data
/// (good for files or heavy network stream).
pub struct Decoder<'a, R> {
    reader: zio::Reader<R, raw::Decoder<'a>>,
}

/// An encoder that compress input data from another `Read`.
pub struct Encoder<'a, R> {
    reader: zio::Reader<R, raw::Encoder<'a>>,
}

impl<R: Read> Decoder<'static, BufReader<R>> {
    /// Creates a new decoder.
    pub fn new(reader: R) -> io::Result<Self> {
        let buffer_size = zstd_safe::DCtx::in_size();

        Self::with_buffer(BufReader::with_capacity(buffer_size, reader))
    }
}

impl<R: BufRead> Decoder<'static, R> {
    /// Creates a new decoder around a `BufRead`.
    pub fn with_buffer(reader: R) -> io::Result<Self> {
        Self::with_dictionary(reader, &[])
    }
    /// Creates a new decoder, using an existing dictionary.
    ///
    /// The dictionary must be the same as the one used during compression.
    pub fn with_dictionary(reader: R, dictionary: &[u8]) -> io::Result<Self> {
        let decoder = raw::Decoder::with_dictionary(dictionary)?;
        let reader = zio::Reader::new(reader, decoder);

        Ok(Decoder { reader })
    }
}

/// Read and discard `bytes_count` bytes in the reader.
#[cfg(feature = "experimental")]
fn consume<R: Read + ?Sized>(this: &mut R, mut bytes_count: usize) -> io::Result<()> {
    let mut buf = [0; 100];
    while bytes_count > 0 {
        let end = min(buf.len(), bytes_count);
        match this.read(&mut buf[..end]) {
            Ok(0) => break,
            Ok(n) => bytes_count -= n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {},
            Err(e) => return Err(e),
        }
    }
    if bytes_count > 0 {
        Err(io::Error::new(io::ErrorKind::UnexpectedEof, "failed to fill whole buffer"))
    } else {
        Ok(())
    }
}

#[cfg(feature = "experimental")]
impl<'a, R: Read + Seek> Decoder<'a, BufReader<R>> {
    fn read_skippable_frame_size(&mut self) -> io::Result<usize> {
        let mut magic_buffer = [0u8; U32_SIZE];
        self.reader.reader_mut().read_exact(&mut magic_buffer)?;

        // Read skippable frame size.
        let mut buffer = [0u8; U32_SIZE];
        self.reader.reader_mut().read_exact(&mut buffer)?;
        let content_size = u32::from_le_bytes(buffer) as usize;

        self.seek_back(U32_SIZE * 2);

        Ok(content_size + SKIPPABLEHEADERSIZE as usize)
    }

    fn seek_back(&mut self, bytes_count: usize) {
        if let Err(error) = self.reader.reader_mut().seek(SeekFrom::Current(-(bytes_count as i64))) {
            panic!("Error while seeking back to the start: {}", error);
        }
    }

    /// Attempt to read a skippable frame and write its content to `dest`.
    /// If it cannot read a skippable frame, the reader will be back to its starting position.
    pub fn read_skippable_frame<W: Write>(&mut self, dest: &mut W) -> io::Result<(usize, MagicVariant)> {
        let magic_buffer = self.reader.peek_4bytes()?;
        let op = self.reader.operation();
        if !op.is_skippable_frame(&magic_buffer) {
            return Err(io::Error::new(io::ErrorKind::Other, "Unsupported frame parameter"));
        }
        self.reader.clear_peeked_data();

        let magic_number = u32::from_le_bytes(magic_buffer);

        // Read skippable frame size.
        let mut buffer = [0u8; U32_SIZE];
        self.reader.reader_mut().read_exact(&mut buffer)?;
        let content_size = u32::from_le_bytes(buffer) as usize;

        if content_size > 0 {
            std::io::copy(&mut self.reader.reader_mut().take(content_size as u64), dest)?;
        }

        let magic_variant = magic_number - MAGIC_SKIPPABLE_START;

        Ok((content_size, MagicVariant(magic_variant as u8)))
    }

    fn get_block_size(&mut self) -> io::Result<(usize, bool)> {
        let mut buffer = [0u8; U24_SIZE];
        self.reader.reader_mut().read_exact(&mut buffer)?;
        let buffer = [buffer[0], buffer[1], buffer[2], 0];
        let block_header = u32::from_le_bytes(buffer);
        let compressed_size = block_header >> 3;
        let last_block = block_header & 1;
        self.seek_back(U24_SIZE);
        Ok((compressed_size as usize, last_block != 0))
    }

    fn find_frame_compressed_size(&mut self) -> io::Result<usize> {
        const ZSTD_BLOCK_HEADER_SIZE: usize = 3;

        // TODO: should we support legacy format?
        let mut magic_buffer = [0u8; U32_SIZE];
        if self.reader.peeking() {
            magic_buffer = self.reader.peeked_data();
            self.reader.clear_peeked_data();
        }
        else {
            self.reader.reader_mut().read_exact(&mut magic_buffer)?;
        }
        let magic_number = u32::from_le_bytes(magic_buffer);
        self.seek_back(U32_SIZE);
        if magic_number & MAGIC_SKIPPABLE_MASK == MAGIC_SKIPPABLE_START {
            self.read_skippable_frame_size()
        }
        else {
            let mut bytes_read = 0;
            // Extract frame header.
            let (header_size, checksum_flag) = self.frame_header_size()?;
            bytes_read += header_size;
            consume(self.reader.reader_mut(), header_size)?;

            // Iterator over each block.
            loop {
                let (compressed_size, last_block) = self.get_block_size()?;
                let block_size = ZSTD_BLOCK_HEADER_SIZE + compressed_size;
                consume(self.reader.reader_mut(), block_size)?;
                bytes_read += block_size;
                if last_block {
                    break;
                }
            }

            self.seek_back(bytes_read);

            if checksum_flag {
                bytes_read += 4;
            }

            Ok(bytes_read)
        }
    }

    fn frame_header_size(&mut self) -> io::Result<(usize, bool)> {
        use crate::map_error_code;
        const MAX_FRAME_HEADER_SIZE_PREFIX: usize = 5;
        let mut buffer = [0u8; MAX_FRAME_HEADER_SIZE_PREFIX];
        self.reader.reader_mut().read_exact(&mut buffer)?;
        let size = frame_header_size(&buffer)
            .map_err(map_error_code)?;
        let byte = buffer[MAX_FRAME_HEADER_SIZE_PREFIX - 1];
        let checksum_flag = (byte >> 2) & 1;
        self.seek_back(MAX_FRAME_HEADER_SIZE_PREFIX);
        Ok((size, checksum_flag != 0))
    }

    /// Skip over a frame, without decompressing it.
    pub fn skip_frame(&mut self) -> io::Result<()> {
        let size = self.find_frame_compressed_size()?;
        consume(self.reader.reader_mut(), size)?;
        Ok(())
    }
}

impl<'a, R: BufRead> Decoder<'a, R> {
    /// Creates a new decoder which employs the provided context for deserialization.
    pub fn with_context(
        reader: R,
        context: &'a mut zstd_safe::DCtx<'static>,
    ) -> Self {
        Self {
            reader: zio::Reader::new(
                reader,
                raw::Decoder::with_context(context),
            ),
        }
    }

    /// Sets this `Decoder` to stop after the first frame.
    ///
    /// By default, it keeps concatenating frames until EOF is reached.
    #[must_use]
    pub fn single_frame(mut self) -> Self {
        self.reader.set_single_frame();
        self
    }

    /// Creates a new decoder, using an existing `DecoderDictionary`.
    ///
    /// The dictionary must be the same as the one used during compression.
    pub fn with_prepared_dictionary<'b>(
        reader: R,
        dictionary: &DecoderDictionary<'b>,
    ) -> io::Result<Self>
    where
        'b: 'a,
    {
        let decoder = raw::Decoder::with_prepared_dictionary(dictionary)?;
        let reader = zio::Reader::new(reader, decoder);

        Ok(Decoder { reader })
    }

    /// Creates a new decoder, using a ref prefix.
    ///
    /// The prefix must be the same as the one used during compression.
    pub fn with_ref_prefix<'b>(
        reader: R,
        ref_prefix: &'b [u8]
    ) -> io::Result<Self>
    where
        'b: 'a,
    {
        let decoder = raw::Decoder::with_ref_prefix(ref_prefix)?;
        let reader = zio::Reader::new(reader, decoder);

        Ok(Decoder { reader })
    }

    /// Recommendation for the size of the output buffer.
    pub fn recommended_output_size() -> usize {
        zstd_safe::DCtx::out_size()
    }

    /// Acquire a reference to the underlying reader.
    pub fn get_ref(&self) -> &R {
        self.reader.reader()
    }

    /// Acquire a mutable reference to the underlying reader.
    ///
    /// Note that mutation of the reader may result in surprising results if
    /// this decoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut R {
        self.reader.reader_mut()
    }

    /// Return the inner `Read`.
    ///
    /// Calling `finish()` is not *required* after reading a stream -
    /// just use it if you need to get the `Read` back.
    pub fn finish(self) -> R {
        self.reader.into_inner()
    }

    crate::decoder_common!(reader);
}

impl<R: BufRead> Read for Decoder<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

impl<R: Read> Encoder<'static, BufReader<R>> {
    /// Creates a new encoder.
    pub fn new(reader: R, level: i32) -> io::Result<Self> {
        let buffer_size = zstd_safe::CCtx::in_size();

        Self::with_buffer(BufReader::with_capacity(buffer_size, reader), level)
    }
}

impl<R: BufRead> Encoder<'static, R> {
    /// Creates a new encoder around a `BufRead`.
    pub fn with_buffer(reader: R, level: i32) -> io::Result<Self> {
        Self::with_dictionary(reader, level, &[])
    }

    /// Creates a new encoder, using an existing dictionary.
    ///
    /// The dictionary must be the same as the one used during compression.
    pub fn with_dictionary(
        reader: R,
        level: i32,
        dictionary: &[u8],
    ) -> io::Result<Self> {
        let encoder = raw::Encoder::with_dictionary(level, dictionary)?;
        let reader = zio::Reader::new(reader, encoder);

        Ok(Encoder { reader })
    }
}

impl<'a, R: BufRead> Encoder<'a, R> {
    /// Creates a new encoder, using an existing `EncoderDictionary`.
    ///
    /// The dictionary must be the same as the one used during compression.
    pub fn with_prepared_dictionary<'b>(
        reader: R,
        dictionary: &EncoderDictionary<'b>,
    ) -> io::Result<Self>
    where
        'b: 'a,
    {
        let encoder = raw::Encoder::with_prepared_dictionary(dictionary)?;
        let reader = zio::Reader::new(reader, encoder);

        Ok(Encoder { reader })
    }

    /// Recommendation for the size of the output buffer.
    pub fn recommended_output_size() -> usize {
        zstd_safe::CCtx::out_size()
    }

    /// Acquire a reference to the underlying reader.
    pub fn get_ref(&self) -> &R {
        self.reader.reader()
    }

    /// Acquire a mutable reference to the underlying reader.
    ///
    /// Note that mutation of the reader may result in surprising results if
    /// this encoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut R {
        self.reader.reader_mut()
    }

    /// Flush any internal buffer.
    ///
    /// This ensures all input consumed so far is compressed.
    ///
    /// Since it prevents bundling currently buffered data with future input,
    /// it may affect compression ratio.
    ///
    /// * Returns the number of bytes written to `out`.
    /// * Returns `Ok(0)` when everything has been flushed.
    pub fn flush(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.reader.flush(out)
    }

    /// Return the inner `Read`.
    ///
    /// Calling `finish()` is not *required* after reading a stream -
    /// just use it if you need to get the `Read` back.
    pub fn finish(self) -> R {
        self.reader.into_inner()
    }

    crate::encoder_common!(reader);
}

impl<R: BufRead> Read for Encoder<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

fn _assert_traits() {
    use std::io::Cursor;

    fn _assert_send<T: Send>(_: T) {}

    _assert_send(Decoder::new(Cursor::new(Vec::new())));
    _assert_send(Encoder::new(Cursor::new(Vec::new()), 1));
}
