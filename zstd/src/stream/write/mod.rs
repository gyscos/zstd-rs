//! Implement push-based [`Write`] trait for both compressing and decompressing.
use std::io::{self, Write};

#[cfg(feature = "tokio")]
use futures::Poll;
#[cfg(feature = "tokio")]
use tokio_io::AsyncWrite;

use zstd_safe;

use crate::dict::{DecoderDictionary, EncoderDictionary};
use crate::stream::{raw, zio};

#[cfg(test)]
#[cfg(feature = "tokio")]
mod async_tests;

#[cfg(test)]
mod tests;

/// An encoder that compress and forward data to another writer.
///
/// This allows to compress a stream of data
/// (good for files or heavy network stream).
///
/// Don't forget to call [`finish()`] before dropping it!
///
/// Note: The zstd library has its own internal input buffer (~128kb).
///
/// [`finish()`]: #method.finish
pub struct Encoder<W: Write> {
    // output writer (compressed data)
    writer: zio::Writer<W, raw::Encoder>,
}

/// A decoder that decompress and forward data to another writer.
pub struct Decoder<W: Write> {
    // output writer (decompressed data)
    writer: zio::Writer<W, raw::Decoder>,
}

/// A wrapper around an `Encoder<W>` that finishes the stream on drop.
pub struct AutoFinishEncoder<W: Write> {
    // We wrap this in an option to take it during drop.
    encoder: Option<Encoder<W>>,

    // TODO: make this a FnOnce once it works in a Box
    on_finish: Option<Box<dyn FnMut(io::Result<W>)>>,
}

impl<W: Write> AutoFinishEncoder<W> {
    fn new<F>(encoder: Encoder<W>, on_finish: F) -> Self
    where
        F: 'static + FnMut(io::Result<W>),
    {
        AutoFinishEncoder {
            encoder: Some(encoder),
            on_finish: Some(Box::new(on_finish)),
        }
    }

    /// Acquires a reference to the underlying writer.
    pub fn get_ref(&self) -> &W {
        self.encoder.as_ref().unwrap().get_ref()
    }

    /// Acquires a mutable reference to the underlying writer.
    ///
    /// Note that mutation of the writer may result in surprising results if
    /// this encoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut W {
        self.encoder.as_mut().unwrap().get_mut()
    }
}

impl<W: Write> Drop for AutoFinishEncoder<W> {
    fn drop(&mut self) {
        let result = self.encoder.take().unwrap().finish();
        if let Some(mut on_finish) = self.on_finish.take() {
            on_finish(result);
        }
    }
}

impl<W: Write> Write for AutoFinishEncoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.encoder.as_mut().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.encoder.as_mut().unwrap().flush()
    }
}

impl<W: Write> Encoder<W> {
    /// Creates a new encoder.
    ///
    /// `level`: compression level (1-21).
    ///
    /// A level of `0` uses zstd's default (currently `3`).
    pub fn new(writer: W, level: i32) -> io::Result<Self> {
        Self::with_dictionary(writer, level, &[])
    }

    /// Creates a new encoder, using an existing dictionary.
    ///
    /// (Provides better compression ratio for small files,
    /// but requires the dictionary to be present during decompression.)
    ///
    /// A level of `0` uses zstd's default (currently `3`).
    pub fn with_dictionary(
        writer: W,
        level: i32,
        dictionary: &[u8],
    ) -> io::Result<Self> {
        let encoder = raw::Encoder::with_dictionary(level, dictionary)?;
        let writer = zio::Writer::new(writer, encoder);
        Ok(Encoder { writer })
    }

    /// Creates a new encoder, using an existing prepared `EncoderDictionary`.
    ///
    /// (Provides better compression ratio for small files,
    /// but requires the dictionary to be present during decompression.)
    pub fn with_prepared_dictionary(
        writer: W,
        dictionary: &EncoderDictionary<'_>,
    ) -> io::Result<Self> {
        let encoder = raw::Encoder::with_prepared_dictionary(dictionary)?;
        let writer = zio::Writer::new(writer, encoder);
        Ok(Encoder { writer })
    }

    /// Returns a wrapper around `self` that will finish the stream on drop.
    ///
    /// # Panic
    ///
    /// Panics on drop if an error happens when finishing the stream.
    pub fn auto_finish(self) -> AutoFinishEncoder<W> {
        self.on_finish(|result| {
            result.unwrap();
        })
    }

    /// Returns an encoder that will finish the stream on drop.
    ///
    /// Calls the given callback with the result from `finish()`.
    pub fn on_finish<F: 'static + FnMut(io::Result<W>)>(
        self,
        f: F,
    ) -> AutoFinishEncoder<W> {
        AutoFinishEncoder::new(self, f)
    }

    /// Acquires a reference to the underlying writer.
    pub fn get_ref(&self) -> &W {
        self.writer.writer()
    }

    /// Acquires a mutable reference to the underlying writer.
    ///
    /// Note that mutation of the writer may result in surprising results if
    /// this encoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut W {
        self.writer.writer_mut()
    }

    /// **Required**: Finishes the stream.
    ///
    /// You *need* to finish the stream when you're done writing, either with
    /// this method or with [`try_finish(self)`](#method.try_finish).
    ///
    /// This returns the inner writer in case you need it.
    ///
    /// To get back `self` in case an error happened, use `try_finish`.
    ///
    /// **Note**: If you don't want (or can't) call `finish()` manually after
    ///           writing your data, consider using `auto_finish()` to get an
    ///           `AutoFinishEncoder`.
    pub fn finish(self) -> io::Result<W> {
        self.try_finish().map_err(|(_, err)| err)
    }

    /// **Required**: Attempts to finish the stream.
    ///
    /// You *need* to finish the stream when you're done writing, either with
    /// this method or with [`finish(self)`](#method.finish).
    ///
    /// This returns the inner writer if the finish was successful, or the
    /// object plus an error if it wasn't.
    ///
    /// `write` on this object will panic after `try_finish` has been called,
    /// even if it fails.
    pub fn try_finish(mut self) -> Result<W, (Self, io::Error)> {
        match self.writer.finish() {
            // Return the writer, because why not
            Ok(()) => Ok(self.writer.into_inner().0),
            Err(e) => Err((self, e)),
        }
    }

    /// Attemps to finish the stream.
    ///
    /// You *need* to finish the stream when you're done writing, either with
    /// this method or with [`finish(self)`](#method.finish).
    pub fn do_finish(&mut self) -> io::Result<()> {
        self.writer.finish()
    }

    /// Return a recommendation for the size of data to write at once.
    pub fn recommended_input_size() -> usize {
        zstd_safe::cstream_in_size()
    }

    /// Controls whether zstd should include a content checksum at the end of each frame.
    pub fn include_checksum(
        &mut self,
        include_checksum: bool,
    ) -> io::Result<()> {
        self.writer.operation_mut().set_parameter(
            zstd_safe::CParameter::ChecksumFlag(include_checksum),
        )
    }

    /// Enables multithreaded compression
    ///
    /// * If `n_workers == 0` (default), then multithreaded will be disabled.
    /// * If `n_workers >= 1`, then compression will be done in separate threads.
    ///   So even `n_workers = 1` may increase performance by separating IO and compression.
    pub fn multithread(&mut self, n_workers: u32) -> io::Result<()> {
        self.writer
            .operation_mut()
            .set_parameter(zstd_safe::CParameter::NbWorkers(n_workers))
    }
}

impl<W: Write> Write for Encoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

#[cfg(feature = "tokio")]
impl<W: AsyncWrite> AsyncWrite for Encoder<W> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        use tokio_io::try_nb;

        try_nb!(self.do_finish());
        self.writer.writer_mut().shutdown()
    }
}

impl<W: Write> Decoder<W> {
    /// Creates a new decoder.
    pub fn new(writer: W) -> io::Result<Self> {
        Self::with_dictionary(writer, &[])
    }

    /// Creates a new decoder, using an existing dictionary.
    ///
    /// (Provides better compression ratio for small files,
    /// but requires the dictionary to be present during decompression.)
    pub fn with_dictionary(writer: W, dictionary: &[u8]) -> io::Result<Self> {
        let decoder = raw::Decoder::with_dictionary(dictionary)?;
        let writer = zio::Writer::new(writer, decoder);
        Ok(Decoder { writer })
    }

    /// Creates a new decoder, using an existing prepared `DecoderDictionary`.
    ///
    /// (Provides better compression ratio for small files,
    /// but requires the dictionary to be present during decompression.)
    pub fn with_prepared_dictionary(
        writer: W,
        dictionary: &DecoderDictionary<'_>,
    ) -> io::Result<Self> {
        let decoder = raw::Decoder::with_prepared_dictionary(dictionary)?;
        let writer = zio::Writer::new(writer, decoder);
        Ok(Decoder { writer })
    }

    /// Acquires a reference to the underlying writer.
    pub fn get_ref(&self) -> &W {
        self.writer.writer()
    }

    /// Acquires a mutable reference to the underlying writer.
    ///
    /// Note that mutation of the writer may result in surprising results if
    /// this decoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut W {
        self.writer.writer_mut()
    }

    /// Returns the inner `Write`.
    pub fn into_inner(self) -> W {
        self.writer.into_inner().0
    }

    /// Return a recommendation for the size of data to write at once.
    pub fn recommended_input_size() -> usize {
        zstd_safe::dstream_in_size()
    }
}

impl<W: Write> Write for Decoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

#[cfg(feature = "tokio")]
impl<W: AsyncWrite> AsyncWrite for Decoder<W> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        self.writer.writer_mut().shutdown()
    }
}

fn _assert_traits() {
    fn _assert_send<T: Send>(_: T) {}

    _assert_send(Decoder::new(Vec::new()));
    _assert_send(Encoder::new(Vec::new(), 1));
}
