use libc::c_void;
use parse_code;
use std::io::{self, Read};
use zstd_sys;

struct DecoderContext {
    s: *mut zstd_sys::ZSTD_DStream,
}

impl Default for DecoderContext {
    fn default() -> Self {
        DecoderContext { s: unsafe { zstd_sys::ZSTD_createDStream() } }
    }
}

impl Drop for DecoderContext {
    fn drop(&mut self) {
        let code = unsafe { zstd_sys::ZSTD_freeDStream(self.s) };
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

    // `true` if we should stop after the first frame.
    single_frame: bool,
}

impl<R: Read> Decoder<R> {
    /// Creates a new decoder.
    pub fn new(reader: R) -> io::Result<Self> {
        Self::with_dictionary(reader, &[])
    }

    /// Sets this `Decoder` to stop after the first frame.
    ///
    /// By default, it keeps concatenating frames until EOF is reached.
    pub fn single_frame(mut self) -> Self {
        self.single_frame = true;
        self
    }

    /// Creates a new decoder, using an existing dictionary.
    ///
    /// The dictionary must be the same as the one used during compression.
    pub fn with_dictionary(reader: R, dictionary: &[u8]) -> io::Result<Self> {

        let buffer_size = unsafe { zstd_sys::ZSTD_DStreamInSize() };

        let context = DecoderContext::default();
        parse_code(unsafe {
            zstd_sys::ZSTD_initDStream_usingDict(context.s,
                                           dictionary.as_ptr() as *const c_void,
                                           dictionary.len())
        })?;

        let decoder = Decoder {
            reader: reader,
            buffer: Vec::with_capacity(buffer_size),
            offset: 0,
            context: context,
            single_frame: false,
        };

        Ok(decoder)
    }

    fn reinit(&mut self) -> io::Result<()> {
        parse_code(unsafe { zstd_sys::ZSTD_resetDStream(self.context.s) })?;
        Ok(())
    }

    /// Recommendation for the size of the output buffer.
    pub fn recommended_output_size() -> usize {
        unsafe { zstd_sys::ZSTD_DStreamOutSize() }
    }

    /// Acquire a reference to the underlying reader.
    pub fn get_ref(&self) -> &R {
        &self.reader
    }

    /// Acquire a mutable reference to the underlying reader.
    ///
    /// Note that mutation of the reader may result in surprising results if
    /// this decoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Return the inner `Read`.
    ///
    /// Calling `finish()` is not *required* after reading a stream -
    /// just use it if you need to get the `Read` back.
    pub fn finish(self) -> R {
        self.reader
    }

    fn refill_buffer(&mut self, in_buffer: &mut zstd_sys::ZSTD_inBuffer)
                     -> io::Result<bool> {

        // We need moar data!
        // Make a nice clean buffer
        let buffer_size = self.buffer.capacity();
        unsafe {
            self.buffer.set_len(buffer_size);
        }

        // And FILL IT!
        let read = self.reader.read(&mut self.buffer)?;
        unsafe {
            self.buffer.set_len(read);
        }
        in_buffer.pos = 0;
        in_buffer.size = read;
        // So we can't read anything: input is exhausted.

        Ok(read > 0)
    }
}

impl<R: Read> Read for Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {

        let mut in_buffer = zstd_sys::ZSTD_inBuffer {
            src: self.buffer.as_ptr() as *const c_void,
            size: self.buffer.len(),
            pos: self.offset,
        };

        if self.offset > self.buffer.capacity() {
            // If we've reached the end of the frame before,
            // don't even try to read more.
            return Ok(0);
        }

        let mut out_buffer = zstd_sys::ZSTD_outBuffer {
            dst: buf.as_mut_ptr() as *mut c_void,
            size: buf.len(),
            pos: 0,
        };
        while out_buffer.pos != buf.len() {

            let mut input_exhausted = false;

            if in_buffer.pos == in_buffer.size {
                input_exhausted = !self.refill_buffer(&mut in_buffer)?;
            }

            let res = unsafe {
                let code =
                    zstd_sys::ZSTD_decompressStream(self.context.s,
                                              &mut out_buffer as *mut zstd_sys::ZSTD_outBuffer,
                                              &mut in_buffer as *mut zstd_sys::ZSTD_inBuffer);
                parse_code(code)?
            };

            if res > 1 && input_exhausted {
                // zstd keeps asking for more, but we're short on data!
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                                          "incomplete frame"));
            }

            if res == 0 {
                // Remember that we've reached the end of the current frame,
                // so we don't try to read the next one.
                if self.single_frame {
                    in_buffer.pos = self.buffer.capacity() + 1;
                    break;
                } else {
                    if in_buffer.pos == in_buffer.size &&
                       !self.refill_buffer(&mut in_buffer)? {
                        // we're out.
                        in_buffer.pos = self.buffer.capacity() + 1;
                        break;
                    } else {
                        // ?
                        self.reinit()?;
                    }
                }
            }
        }
        self.offset = in_buffer.pos;
        Ok(out_buffer.pos)
    }
}
