use ll;

use ::parse_code;
use std::io::{self, Write};

struct EncoderContext {
    s: *mut ll::ZSTD_CStream,
}

impl Default for EncoderContext {
    fn default() -> Self {
        EncoderContext { s: unsafe { ll::ZSTD_createCStream() } }
    }
}

impl Drop for EncoderContext {
    fn drop(&mut self) {
        let code = unsafe { ll::ZSTD_freeCStream(self.s) };
        parse_code(code).unwrap();
    }
}

/// An encoder that compress and forward data to another writer.
///
/// This allows to compress a stream of data
/// (good for files or heavy network stream).
///
/// Don't forget to call `finish()` before dropping it!
///
/// Note: The zstd library has its own internal input buffer (~128kb).
pub struct Encoder<W: Write> {
    // output writer (compressed data)
    writer: W,
    // output buffer
    buffer: Vec<u8>,
    // offset in the output buffer
    offset: usize,

    // compression context
    context: EncoderContext,
}

/// A wrapper around an `Encoder<W>` that finishes the stream on drop.
pub struct AutoFinishEncoder<W: Write> {
    // We wrap this in an option to take it during drop.
    encoder: Option<Encoder<W>>,
    // TODO: make this a FnOnce once it works in a Box
    on_finish: Option<Box<FnMut(io::Result<W>)>>,
}

impl<W: Write> AutoFinishEncoder<W> {
    fn new<F: 'static + FnMut(io::Result<W>)>(encoder: Encoder<W>,
                                              on_finish: F)
                                              -> Self {
        AutoFinishEncoder {
            encoder: Some(encoder),
            on_finish: Some(Box::new(on_finish)),
        }
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
    /// `level`: compression level (1-21)
    pub fn new(writer: W, level: i32) -> io::Result<Self> {
        Self::with_dictionary(writer, level, &[])
    }

    /// Creates a new encoder, using an existing dictionary.
    ///
    /// (Provides better compression ratio for small files,
    /// but requires the dictionary to be present during decompression.)
    pub fn with_dictionary(writer: W, level: i32, dictionary: &[u8])
                           -> io::Result<Self> {
        let context = EncoderContext::default();

        // Initialize the stream with an existing dictionary
        parse_code(unsafe {
            ll::ZSTD_initCStream_usingDict(context.s,
                                           dictionary.as_ptr(),
                                           dictionary.len(),
                                           level)
        })?;

        Encoder::with_context(writer, context)
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
    pub fn on_finish<F: 'static + FnMut(io::Result<W>)>
        (self, f: F)
         -> AutoFinishEncoder<W> {
        AutoFinishEncoder::new(self, f)
    }

    fn with_context(writer: W, context: EncoderContext) -> io::Result<Self> {
        // This is the output buffer size,
        // for compressed data we get from zstd.
        let buffer_size = unsafe { ll::ZSTD_CStreamOutSize() };

        Ok(Encoder {
            writer: writer,
            buffer: Vec::with_capacity(buffer_size),
            offset: 0,
            context: context,
        })
    }

    /// Finishes the stream. You *need* to call this after writing your stuff.
    ///
    /// This returns the inner writer in case you need it.
    ///
    /// **Note**: If you don't want (or can't) call `finish()` manually after writing your data,
    /// consider using `auto_finish()` to get an `AutoFinishEncoder`.
    pub fn finish(mut self) -> io::Result<W> {

        // First, closes the stream.

        let mut buffer = ll::ZSTD_outBuffer {
            dst: self.buffer.as_mut_ptr(),
            size: self.buffer.capacity(),
            pos: 0,
        };
        let remaining = parse_code(unsafe {
            ll::ZSTD_endStream(self.context.s,
                               &mut buffer as *mut ll::ZSTD_outBuffer)
        })?;
        unsafe {
            self.buffer.set_len(buffer.pos);
        }
        if remaining != 0 {
            // Need to flush?
            panic!("Need to flush, but I'm lazy.");
        }

        // Write the end out
        self.writer.write_all(&self.buffer)?;

        // Return the writer, because why not
        Ok(self.writer)
    }

    /// Return a recommendation for the size of data to write at once.
    pub fn recommended_input_size() -> usize {
        unsafe { ll::ZSTD_CStreamInSize() }
    }
}

impl<W: Write> Write for Encoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {


        if self.offset < self.buffer.len() {
            // If we still had some things to write, do it first.
            self.offset += self.writer.write(&self.buffer[self.offset..])?;
            // Maybe next time!
            return Err(io::Error::new(io::ErrorKind::Interrupted,
                                      "Internal buffer full"));
        }

        // If we get to here, `self.buffer` can safely be discarded.

        let mut in_buffer = ll::ZSTD_inBuffer {
            src: buf.as_ptr(),
            size: buf.len(),
            pos: 0,
        };

        let mut out_buffer = ll::ZSTD_outBuffer {
            dst: self.buffer.as_mut_ptr(),
            size: self.buffer.capacity(),
            pos: 0,
        };


        unsafe {
            // Time to fill our input buffer
            let code = ll::ZSTD_compressStream(self.context.s,
                                                   &mut out_buffer as *mut ll::ZSTD_outBuffer,
                                                   &mut in_buffer as *mut ll::ZSTD_inBuffer);
            // Note: this may very well be empty,
            // if it doesn't exceed zstd's own buffer
            self.buffer.set_len(out_buffer.pos);

            // Do we care about the hint?
            let _ = parse_code(code)?;
        }


        // This is the first time he sees this buffer.
        // Remember his delicate touch.
        self.offset = self.writer.write(&self.buffer)?;

        Ok(in_buffer.pos)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut buffer = ll::ZSTD_outBuffer {
            dst: self.buffer.as_mut_ptr(),
            size: self.buffer.capacity(),
            pos: 0,
        };
        unsafe {
            let code =
                ll::ZSTD_flushStream(self.context.s,
                                     &mut buffer as *mut ll::ZSTD_outBuffer);
            self.buffer.set_len(buffer.pos);
            let _ = parse_code(code)?;
        }

        self.writer.write_all(&self.buffer)?;
        Ok(())
    }
}
