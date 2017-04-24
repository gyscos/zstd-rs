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

// Extra bit of information that is stored along RefillBuffer state.
// It describes the context in which refill was requested.
#[derive(PartialEq, Copy, Clone)]
enum RefillBufferHint {
    // refill was requested during regular read operation,
    // no extra actions are required
    None,
    // we've reached the end of buffer and zstd wants more data
    // in this circumstances refill must return more data, otherwise this is an error
    FailIfEmpty,
    // we've reached the end of current frame,
    // if refill brings more data we'll start new frame and complete reading otherwise
    EndOfFrame,
}

enum DecoderState {
    Completed,
    Active,
    RefillBuffer(RefillBufferHint),
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

    // current state of the decoder
    state: DecoderState,
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
            state: DecoderState::RefillBuffer(RefillBufferHint::None),
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
        let read = self.read_with_retry()?;
        unsafe {
            self.buffer.set_len(read);
        }

        self.offset = 0;
        in_buffer.pos = 0;
        in_buffer.size = read;

        // So we can't read anything: input is exhausted.
        Ok(read > 0)
    }

    fn read_with_retry(&mut self) -> Result<usize, io::Error> {
        loop {
            match self.reader.read(&mut self.buffer) {
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {},
                otherwise => return otherwise
            }
        }
    }
}

impl<R: Read> Decoder<R> {
    /// This function handles buffer_refill state of the read operation
    /// It returns true if read operation should be stopped and false otherwise
    fn handle_refill(&mut self, hint: RefillBufferHint, 
        in_buffer: &mut zstd_sys::ZSTD_inBuffer, out_buffer: &mut zstd_sys::ZSTD_outBuffer)-> Result<bool, io::Error> {

        let refilled = match self.refill_buffer(in_buffer) {
            Err(ref err) if out_buffer.pos > 0 && err.kind() == io::ErrorKind::WouldBlock => {
                // underlying reader was blocked but we've already put some data into the output buffer
                // we need to stop this read operation so data won' be lost
                return Ok(true);
            },
            otherwise => otherwise
        }?;

        match hint {
            RefillBufferHint::None => {
                // can read again
                self.state = DecoderState::Active
            },
            RefillBufferHint::FailIfEmpty => {
                if refilled {
                    // can read again
                    self.state = DecoderState::Active;
                }
                else {
                    // zstd keeps asking for more, but we're short on data!
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "incomplete frame"));
                }
            },
            RefillBufferHint::EndOfFrame => {
                // at the end of frame
                if refilled {
                    // has more data - start new frame
                    self.reinit()?;
                    self.state = DecoderState::Active;
                }
                else {
                    // no more data - we are done
                    self.state = DecoderState::Completed;
                    return Ok(true);
                }
            }
        }

        return Ok(false);
    }

    /// This function handles Active state in the read operation - main read loop.
    /// It returns true if read operation should be stopped and false otherwise
    fn handle_active(&mut self, in_buffer: &mut zstd_sys::ZSTD_inBuffer, out_buffer: &mut zstd_sys::ZSTD_outBuffer) -> Result<bool, io::Error> {

        while out_buffer.pos < out_buffer.size {
            if in_buffer.pos == in_buffer.size {
                self.state = DecoderState::RefillBuffer(RefillBufferHint::None);
                // refill buffer and continue reading
                return Ok(false);
            }

            let is_end_of_frame = unsafe {
                let code =
                    zstd_sys::ZSTD_decompressStream(self.context.s,
                                            out_buffer as *mut zstd_sys::ZSTD_outBuffer,
                                            in_buffer as *mut zstd_sys::ZSTD_inBuffer);
                let res = parse_code(code)?;
                res == 0
            };

            self.offset = in_buffer.pos;

            if in_buffer.pos == in_buffer.size {
                let hint = 
                    if is_end_of_frame { 
                        RefillBufferHint::EndOfFrame 
                    } 
                    else { 
                        RefillBufferHint::FailIfEmpty 
                    };
                // refill buffer and continue
                self.state = DecoderState::RefillBuffer(hint);
                return Ok(false);
            }
            if is_end_of_frame && self.single_frame {
                // at the end of frame and we know that this frame is the only one
                // stop
                self.state = DecoderState::Completed;
                return Ok(true);
            }
        }
        return Ok(true);
    }
}

impl<R: Read> Read for Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {

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
            let should_stop = match self.state {
                DecoderState::Completed => {
                    return Ok(0);
                },
                DecoderState::RefillBuffer(action) => {
                    self.handle_refill(action, &mut in_buffer, &mut out_buffer)?
                },
                DecoderState::Active => {
                    self.handle_active(&mut in_buffer, &mut out_buffer)?
                },
            };
            if should_stop {
                break;
            }
        }

        return Ok(out_buffer.pos);
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

    struct InterruptingReader<T: AsyncRead> {
        counter: u8,
        reader: T,
    }

    impl<T: AsyncRead> InterruptingReader<T> {
        fn new(reader: T)-> InterruptingReader<T> {
            InterruptingReader { counter: 5, reader: reader }
        }
    }

    impl<T: AsyncRead> AsyncRead for InterruptingReader<T> {
    }

    impl<T: AsyncRead> io::Read for InterruptingReader<T> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.counter > 0 {
                self.counter = self.counter  - 1;
                Err(io::Error::from(io::ErrorKind::Interrupted))
            }
            else {
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

    #[test]
    fn test_async_read_interrupt() {
        use stream::encode_all;

        let source = "abc".repeat(1024 * 10).into_bytes();
        let encoded = encode_all(&source[..], 1).unwrap();
        let writer = test_async_read_worker(InterruptingReader::new(&encoded[..]), Cursor::new(Vec::new())).unwrap();
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
