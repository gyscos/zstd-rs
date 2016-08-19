use std::io::{self, Read};

use ll;
use ::parse_code;

struct DecoderContext {
    s: *mut ll::ZSTD_DStream,
}

impl Default for DecoderContext {
    fn default() -> Self {
        DecoderContext { s: unsafe { ll::ZSTD_createDStream() } }
    }
}

impl Drop for DecoderContext {
    fn drop(&mut self) {
        let code = unsafe { ll::ZSTD_freeDStream(self.s) };
        parse_code(code).unwrap();
    }
}


/// A decoder that decompress input data from another `Read`.
///
/// This allows to read a stream of compressed data
/// (good for files or heavy network stream).
pub struct Decoder<R: Read> {
    // input reader (compressed data)
    reader: R,
    // input buffer
    buffer: Vec<u8>,
    // we already read everything in the buffer up to that point
    offset: usize,
    // decompression context
    context: DecoderContext,
}

impl<R: Read> Decoder<R> {
    /// Creates a new decoder.
    pub fn new(reader: R) -> io::Result<Self> {
        let context = DecoderContext::default();

        try!(parse_code(unsafe { ll::ZSTD_initDStream(context.s) }));

        Decoder::with_context(reader, context)
    }

    /// Creates a new decoder, using an existing dictionary.
    ///
    /// The dictionary must be the same as the one used during compression.
    pub fn with_dictionary(reader: R, dictionary: &[u8]) -> io::Result<Self> {
        let context = DecoderContext::default();

        try!(parse_code(unsafe {
            ll::ZSTD_initDStream_usingDict(context.s,
                                           dictionary.as_ptr(),
                                           dictionary.len())
        }));

        Decoder::with_context(reader, context)
    }

    fn with_context(reader: R, context: DecoderContext) -> io::Result<Self> {
        let buffer_size = unsafe { ll::ZSTD_DStreamInSize() };

        Ok(Decoder {
            reader: reader,
            buffer: Vec::with_capacity(buffer_size),
            offset: 0,
            context: context,
        })
    }

    /// Recommendation for the size of the output buffer.
    pub fn recommended_output_size() -> usize {
        unsafe { ll::ZSTD_DStreamOutSize() }
    }

    /// Return the inner `Read`.
    ///
    /// Calling `finish()` is not *required* after reading a stream -
    /// just use it if you need to get the `Read` back.
    pub fn finish(self) -> R {
        self.reader
    }
}

impl<R: Read> Read for Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {

        if self.offset > self.buffer.capacity() {
            // If we've reached the end of the frame before,
            // don't even try to read more.
            return Ok(0);
        }

        let mut out_buffer = ll::ZSTD_outBuffer {
            dst: buf.as_mut_ptr(),
            size: buf.len(),
            pos: 0,
        };
        let mut in_buffer = ll::ZSTD_inBuffer {
            src: self.buffer.as_ptr(),
            size: self.buffer.len(),
            pos: self.offset,
        };
        while out_buffer.pos != buf.len() {

            let mut input_exhausted = false;

            if in_buffer.pos == in_buffer.size {
                // We need moar data!
                // Make a nice clean buffer
                let buffer_size = self.buffer.capacity();
                unsafe {
                    self.buffer.set_len(buffer_size);
                }

                // And FILL IT!
                let read = try!(self.reader.read(&mut self.buffer));
                unsafe {
                    self.buffer.set_len(read);
                }
                in_buffer.pos = 0;
                in_buffer.size = read;
                // So we can't read anything: input is exhausted.
                if read == 0 {
                    input_exhausted = true;
                }
            }

            let res = unsafe {
                let code =
                    ll::ZSTD_decompressStream(self.context.s,
                                              &mut out_buffer as *mut ll::ZSTD_outBuffer,
                                              &mut in_buffer as *mut ll::ZSTD_inBuffer);
                try!(parse_code(code))
            };

            if res > 1 && input_exhausted {
                // zstd keeps asking for more, but we're short on data!
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                                          "incomplete frame"));
            }

            if res == 0 {
                // Remember that we've reached the end of the current frame,
                // so we don't try to read the next one.
                in_buffer.pos = self.buffer.capacity() + 1;
                break;
            }
        }
        self.offset = in_buffer.pos;
        Ok(out_buffer.pos)
    }
}
