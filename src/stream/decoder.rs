use libc::c_void;
use parse_code;
use std::io::{self, Read};

#[cfg(feature = "tokio")]
use tokio_io::AsyncRead;
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

/// Extra bit of information that is stored along `RefillBuffer` state.
/// It describes the context in which refill was requested.
#[derive(PartialEq, Copy, Clone)]
enum RefillBufferHint {
    /// refill was requested during regular read operation,
    /// no extra actions are required
    None,
    /// we've reached the end of buffer and zstd wants more data
    /// in this circumstances refill must return more data, otherwise this is an error
    FailIfEmpty,
    /// we've reached the end of current frame,
    /// if refill brings more data we'll start new frame and complete reading otherwise
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

    // Prepares the context for another stream, whith minimal re-allocation.
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
    //
    // Returns the number of bytes read.
    fn refill_buffer(&mut self, in_buffer: &mut zstd_sys::ZSTD_inBuffer)
                     -> io::Result<usize> {
        // At this point it's safe to discard anything in `self.buffer`.
        // (in_buffer HAS to point to self.buffer)

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

        // If we can't read anything, it means input is exhausted.
        Ok(read)
    }

    // Read and retry on Interrupted errors.
    fn read_with_retry(&mut self) -> Result<usize, io::Error> {
        loop {
            match self.reader.read(&mut self.buffer) {
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                otherwise => return otherwise,
            }
        }
    }
}

impl<R: Read> Decoder<R> {
    /// This function handles buffer_refill state of the read operation
    ///
    /// It returns true if read operation should be stopped and false otherwise
    fn handle_refill(&mut self, hint: RefillBufferHint,
                     in_buffer: &mut zstd_sys::ZSTD_inBuffer,
                     out_buffer: &mut zstd_sys::ZSTD_outBuffer)
                     -> Result<bool, io::Error> {

        // refilled = false if we reached the end of the input.
        let refilled = match self.refill_buffer(in_buffer) {
            Err(ref err) if out_buffer.pos > 0 &&
                            err.kind() == io::ErrorKind::WouldBlock => {
                // The underlying reader was blocked, but we've already
                // put some data into the output buffer.
                // We need to stop this read operation so data won' be lost.
                return Ok(true);
            }
            // We are refilled if we were able to read anything
            otherwise => otherwise.map(|read| read > 0),
        }?;

        match hint {
            RefillBufferHint::None => {
                // Nothing special, we can read again
                self.state = DecoderState::Active
            }
            RefillBufferHint::FailIfEmpty => {
                if refilled {
                    // We can read again
                    self.state = DecoderState::Active;
                } else {
                    // zstd keeps asking for more, but we're short on data!
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof,
                                              "incomplete frame"));
                }
            }
            RefillBufferHint::EndOfFrame => {
                // at the end of frame
                if refilled {
                    // There is data to process - start new frame
                    self.reinit()?;
                    self.state = DecoderState::Active;
                } else {
                    // no more data - we are done
                    self.state = DecoderState::Completed;
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// This function handles Active state in the read operation - main read loop.
    ///
    /// It returns true if read operation should be stopped and false otherwise
    fn handle_active(&mut self, in_buffer: &mut zstd_sys::ZSTD_inBuffer,
                     out_buffer: &mut zstd_sys::ZSTD_outBuffer)
                     -> Result<bool, io::Error> {

        // As long as we can keep writing...
        while out_buffer.pos < out_buffer.size {
            if in_buffer.pos == in_buffer.size {
                self.state =
                    DecoderState::RefillBuffer(RefillBufferHint::None);
                // refill buffer and continue reading
                return Ok(false);
            }

            // Let's use the hint from ZSTD to detect end of frames.
            let is_end_of_frame = unsafe {
                let code =
                    zstd_sys::ZSTD_decompressStream(self.context.s,
                                            out_buffer as *mut zstd_sys::ZSTD_outBuffer,
                                            in_buffer as *mut zstd_sys::ZSTD_inBuffer);
                let res = parse_code(code)?;
                res == 0
            };

            // Record what we've consumed.
            self.offset = in_buffer.pos;

            if is_end_of_frame && self.single_frame {
                // We're at the end of the frame,
                // and we know that this frame is the only one.
                // Stop.
                self.state = DecoderState::Completed;
                return Ok(true);
            }

            if in_buffer.pos == in_buffer.size {
                let hint = if is_end_of_frame {
                    // If the frame is over, it's fine to stop there.
                    RefillBufferHint::EndOfFrame
                } else {
                    // If it's not, then we need more data!
                    RefillBufferHint::FailIfEmpty
                };
                // Refill the buffer and continue
                self.state = DecoderState::RefillBuffer(hint);
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl<R: Read> Read for Decoder<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {

        // Self-contained buffer pointer, size and offset
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
                }
                DecoderState::RefillBuffer(action) => {
                        self.handle_refill(action,
                                           &mut in_buffer,
                                           &mut out_buffer)?
                    }
                DecoderState::Active => {
                    self.handle_active(&mut in_buffer, &mut out_buffer)?
                }
            };
            if should_stop {
                break;
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
    use futures::Future;
    use partial_io::{GenInterruptedWouldBlock, PartialAsyncRead,
                     PartialWithErrors};
    use quickcheck::quickcheck;
    use std::io::{self, Cursor};
    use tokio_io::{AsyncWrite, AsyncRead, io as tokio_io};

    #[test]
    fn test_async_read() {
        use stream::encode_all;

        let source = "abc".repeat(1024 * 10).into_bytes();
        let encoded = encode_all(&source[..], 1).unwrap();
        let writer = test_async_read_worker(&encoded[..],
                                            Cursor::new(Vec::new()))
                .unwrap();
        let output = writer.into_inner();
        assert_eq!(source, output);
    }

    #[test]
    fn test_async_read_partial() {
        quickcheck(test as fn(_) -> _);

        fn test(encode_ops: PartialWithErrors<GenInterruptedWouldBlock>) {
            use stream::encode_all;

            let source = "abc".repeat(1024 * 10).into_bytes();
            let encoded = encode_all(&source[..], 1).unwrap();
            let reader = PartialAsyncRead::new(&encoded[..], encode_ops);
            let writer = test_async_read_worker(reader,
                                                Cursor::new(Vec::new()))
                    .unwrap();
            let output = writer.into_inner();
            assert_eq!(source, output);
        }
    }

    fn test_async_read_worker<R: AsyncRead, W: AsyncWrite>
        (r: R, w: W)
         -> io::Result<W> {
        use super::Decoder;

        let decoder = Decoder::new(r).unwrap();
        let (_, _, w) = try!(tokio_io::copy(decoder, w).wait());
        Ok(w)
    }
}
