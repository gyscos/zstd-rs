use libc::c_void;
use parse_code;
use std::io::{self, Read};
use zstd_sys;

#[cfg(feature = "tokio")]
use tokio_io::AsyncRead;

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

    // 'false' if end of frame was not yet reached
    end_of_frame: bool,
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
            end_of_frame: false,
        };

        Ok(decoder)
    }

    fn reinit(&mut self) -> io::Result<()> {
        parse_code(unsafe { zstd_sys::ZSTD_resetDStream(self.context.s) })?;
        self.end_of_frame = false;
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

    // Attemps to refill the input buffer.
    fn refill_buffer(&mut self, in_buffer: &mut zstd_sys::ZSTD_inBuffer)
                     -> io::Result<bool> {

        // We need moar data!
        // Make a nice clean buffer
        let buffer_size = self.buffer.capacity();
        unsafe {
            self.buffer.set_len(buffer_size);
        }

        // And FILL IT!
        let read = self.reader.read(&mut self.buffer).map_err(|err| {
            unsafe {
                // in case of error reset buffer length and offset in buffer to zero
                // so subsequent read attempt will try to refill the buffer
                self.offset = 0;
                self.buffer.set_len(0);
            }
            err
        })?;
        unsafe {
            self.buffer.set_len(read);
        }
        in_buffer.pos = 0;
        in_buffer.size = read;
        // So we can't read anything: input is exhausted.

        Ok(read > 0)
    }

    fn mark_completed(&mut self) {
        self.offset = self.buffer.capacity() + 1;
    }

    fn is_completed(&self) -> bool {
        return self.offset == self.buffer.capacity() + 1;
    }
}

impl<R: Read> Read for Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {

        if self.is_completed() {
            // If we've reached the end of the frame before,
            // don't even try to read more.
            return Ok(0);
        }

        let mut in_buffer = zstd_sys::ZSTD_inBuffer {
            src: self.buffer.as_ptr() as *const c_void,
            size: self.buffer.len(),
            pos: self.offset,
        };

        let mut out_buffer = zstd_sys::ZSTD_outBuffer {
            dst: buf.as_mut_ptr() as *mut c_void,
            size: buf.len(),
            pos: 0,
        };

        loop {
            if out_buffer.pos == buf.len() {
                // receiver buffer is filled
                // remember last written position
                self.offset = in_buffer.pos;
                break;
            }
            if self.single_frame && self.end_of_frame {
                self.mark_completed();
                break;
            }

            let input_exhausted = 
                in_buffer.pos == in_buffer.size && 
                match self.refill_buffer(&mut in_buffer) {
                    Ok(n) => !n,
                    Err(ref err) if out_buffer.pos > 0 && err.kind() == io::ErrorKind::WouldBlock => {
                        // if we get here then:
                        // - inner buffer is empty
                        // - we've already written something to the receiver buffer
                        // in this case we need to report all data that was already in out_buffer
                        break;
                    },
                    Err(e) => return Err(e)
                };

            if self.end_of_frame {
                if input_exhausted {
                    self.mark_completed();
                    break;
                }
                else {
                    self.reinit()?;
                }
            }

            // save value of end_of_frame in case if next call to refill_buffer will fail
            self.end_of_frame = unsafe {
                let code =
                    zstd_sys::ZSTD_decompressStream(self.context.s,
                                              &mut out_buffer as *mut zstd_sys::ZSTD_outBuffer,
                                              &mut in_buffer as *mut zstd_sys::ZSTD_inBuffer);
                let res = parse_code(code)?;
                res == 0
            };

            if !self.end_of_frame && input_exhausted {
                // zstd keeps asking for more, but we're short on data!
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                                          "incomplete frame"));
            }
        }
        Ok(out_buffer.pos)
    }
}

#[cfg(feature = "tokio")]
impl<R: AsyncRead> AsyncRead for Decoder<R> {
    unsafe fn prepare_uninitialized_buffer(&self, _buf: &mut [u8]) -> bool {
        false
    }
}

#[cfg(test)]
#[cfg(feature = "tokio")]
mod async_tests {
    use std::io::{self, Cursor};
    use tokio_io::{AsyncWrite, AsyncRead, io as tokio_io};
    use futures::{Future, task};

    struct BlockingReader<T: AsyncRead> {
        block: bool,
        reader: T,
    }

    impl<T: AsyncRead> BlockingReader<T> {
        fn new(reader: T)-> BlockingReader<T> {
            BlockingReader { block: false, reader: reader }
        }
    }

    impl<T: AsyncRead> AsyncRead for BlockingReader<T> {
    }

    impl<T: AsyncRead> io::Read for BlockingReader<T> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.block {
                self.block = false;
                task::park().unpark();
                Err(io::Error::from(io::ErrorKind::WouldBlock))
            }
            else {
                self.block = true;
                self.reader.read(buf)
            }
        }
    }

    #[test]
    fn test_async_read() {
        use stream::encode_all;

        let source = "abc".repeat(1024 * 10).into_bytes();
        let encoded = encode_all(&source[..], 1).unwrap();
        let writer = test_async_read_worker(&encoded[..], Cursor::new(Vec::new())).unwrap();
        let output = writer.into_inner();
        assert_eq!(source, output);
    }

    #[test]
    fn test_async_read_block() {
        use stream::encode_all;

        let source = "abc".repeat(1024 * 10).into_bytes();
        let encoded = encode_all(&source[..], 1).unwrap();
        let writer = test_async_read_worker(BlockingReader::new(&encoded[..]), Cursor::new(Vec::new())).unwrap();
        let output = writer.into_inner();
        assert_eq!(source, output);
    }

    fn test_async_read_worker<R: AsyncRead, W: AsyncWrite>(r: R, w: W) -> io::Result<W> {
        use super::Decoder;

        let decoder = Decoder::new(r).unwrap();
        let (_, _, w) = try!(tokio_io::copy(decoder, w).wait());
        Ok(w)
    }
}
