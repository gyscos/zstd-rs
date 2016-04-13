use std::io::{self, Write};

use ll;

struct EncoderContext {
    c: ll::ZBUFFCompressionContext,
}

impl EncoderContext {
    fn new() -> Self {
        EncoderContext { c: unsafe { ll::ZBUFF_createCCtx() } }
    }
}

impl Drop for EncoderContext {
    fn drop(&mut self) {
        let code = unsafe { ll::ZBUFF_freeCCtx(self.c) };
        ll::parse_code(code).unwrap();
    }
}


/// An encoder that compress and forward data to another writer.
///
/// Don't forget to call `finish()` before dropping it!
///
/// Note: The zstd library has its own internal input buffer (~128kb).
pub struct Encoder<W: Write> {
    // output writer (compressed data)
    writer: W,
    // output buffer
    buffer: Vec<u8>,

    // compression context
    context: EncoderContext,
}

impl<W: Write> Encoder<W> {
    /// Creates a new encoder.
    ///
    /// `level`: compression level (1-21)
    pub fn new(writer: W, level: i32) -> io::Result<Self> {
        let context = EncoderContext::new();

        // Initialize the stream
        try!(ll::parse_code(unsafe {
            ll::ZBUFF_compressInit(context.c, level)
        }));

        Encoder::with_context(writer, context)
    }

    /// Creates a new encoder, using an existing dictionary.
    ///
    /// (Provides better compression ratio for small files,
    /// but requires the dictionary to be present during decompression.)
    pub fn with_dictionary(writer: W, level: i32, dictionary: &[u8])
                           -> io::Result<Self> {
        let context = EncoderContext::new();

        // Initialize the stream with an existing dictionary
        try!(ll::parse_code(unsafe {
            ll::ZBUFF_compressInitDictionary(context.c,
                                             dictionary.as_ptr(),
                                             dictionary.len(),
                                             level)
        }));

        Encoder::with_context(writer, context)
    }

    fn with_context(writer: W, context: EncoderContext) -> io::Result<Self> {
        // This is the output buffer size, for compressed data we get from zstd.
        let buffer_size = unsafe { ll::ZBUFF_recommendedCOutSize() };

        Ok(Encoder {
            writer: writer,
            buffer: Vec::with_capacity(buffer_size),
            context: context,
        })
    }

    /// Finishes the stream. You *need* to call this after writing your stuff.
    ///
    /// This returns the inner writer in case you need it.
    pub fn finish(mut self) -> io::Result<W> {

        // First, closes the stream.
        let mut out_size = self.buffer.capacity();
        let remaining = try!(ll::parse_code(unsafe {
            ll::ZBUFF_compressEnd(self.context.c,
                                  self.buffer.as_mut_ptr(),
                                  &mut out_size)
        }));
        unsafe {
            self.buffer.set_len(out_size);
        }
        if remaining != 0 {
            // Need to flush?
            panic!("Need to flush, but I'm lazy.");
        }

        // Write the end out
        try!(self.writer.write_all(&self.buffer));

        // Return the writer, because why not
        Ok(self.writer)
    }

    /// Return a recommendation for the size of data to write at once.
    pub fn recommended_input_size() -> usize {
        unsafe { ll::ZBUFF_recommendedCInSize() }
    }
}

impl<W: Write> Write for Encoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // How much we've read from this task
        let mut read = 0;
        while read != buf.len() {
            let mut out_size = self.buffer.capacity();
            let mut in_size = buf.len() - read;

            unsafe {
                // Compress the given buffer into our output buffer
                let code = ll::ZBUFF_compressContinue(self.context.c,
                                                      self.buffer
                                                          .as_mut_ptr(),
                                                      &mut out_size,
                                                      buf[read..].as_ptr(),
                                                      &mut in_size);
                self.buffer.set_len(out_size);

                // Do we care about the hint?
                let _ = try!(ll::parse_code(code));
            }
            try!(self.writer.write_all(&self.buffer));
            read += in_size;
        }
        Ok(read)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut out_size = self.buffer.capacity();
        let written = try!(ll::parse_code(unsafe {
            ll::ZBUFF_compressFlush(self.context.c,
                                    self.buffer.as_mut_ptr(),
                                    &mut out_size)
        }));
        unsafe {
            self.buffer.set_len(written);
        }

        try!(self.writer.write_all(&self.buffer));
        Ok(())
    }
}
