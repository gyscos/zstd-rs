use libc::c_void;

use parse_code;
use std::io::{self, Write};
use zstd_sys;

#[cfg(feature = "tokio")]
use futures::Poll;
#[cfg(feature = "tokio")]
use tokio_io::AsyncWrite;

struct EncoderContext {
    s: *mut zstd_sys::ZSTD_CStream,
}

impl Default for EncoderContext {
    fn default() -> Self {
        EncoderContext { s: unsafe { zstd_sys::ZSTD_createCStream() } }
    }
}

impl Drop for EncoderContext {
    fn drop(&mut self) {
        let code = unsafe { zstd_sys::ZSTD_freeCStream(self.s) };
        parse_code(code).unwrap();
    }
}

#[derive(PartialEq)]
enum EncoderState {
    Accepting,
    // Finished just makes sure writes are no longer accepted.
    Finished,
    StreamEnd,
    // The "stream ended and flushed" state corresponds to the object being
    // destroyed.
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
    state: EncoderState,
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
            zstd_sys::ZSTD_initCStream_usingDict(context.s,
                                           dictionary.as_ptr() as *const c_void,
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
        self.on_finish(|result| { result.unwrap(); })
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
        let buffer_size = unsafe { zstd_sys::ZSTD_CStreamOutSize() };

        Ok(Encoder {
               writer: writer,
               buffer: Vec::with_capacity(buffer_size),
               offset: 0,
               context: context,
               state: EncoderState::Accepting,
           })
    }

    /// Acquires a reference to the underlying writer.
    pub fn get_ref(&self) -> &W {
        &self.writer
    }

    /// Acquires a mutable reference to the underlying writer.
    ///
    /// Note that mutation of the writer may result in surprising results if
    /// this encoder is continued to be used.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.writer
    }

    /// Finishes the stream. You *need* to call this after writing your stuff.
    ///
    /// This returns the inner writer in case you need it.
    ///
    /// To get back `self` in case an error happened, use `try_finish`.
    ///
    /// **Note**: If you don't want (or can't) call `finish()` manually after writing your data,
    /// consider using `auto_finish()` to get an `AutoFinishEncoder`.
    pub fn finish(mut self) -> io::Result<W> {
        self.do_finish()?;
        // Return the writer, because why not
        Ok(self.writer)
    }

    /// Attempts to finish the stream. You *need* to call this after writing your stuff.
    ///
    /// This returns the inner writer if the finish was successful, or the
    /// object plus an error if it wasn't.
    ///
    /// `write` on this object will panic after `try_finish` has been called,
    /// even if it fails.
    pub fn try_finish(mut self) -> Result<W, (Self, io::Error)> {
        match self.do_finish() {
            Ok(()) => Ok(self.writer),
            Err(e) => Err((self, e)),
        }
    }

    fn do_finish(&mut self) -> io::Result<()> {
        if self.state == EncoderState::Accepting {
            // Write any data pending in `self.buffer`.
            self.write_from_offset()?;
            self.state = EncoderState::Finished;
        }

        if self.state == EncoderState::Finished {
            // First, closes the stream.
            let mut buffer = zstd_sys::ZSTD_outBuffer {
                dst: self.buffer.as_mut_ptr() as *mut c_void,
                size: self.buffer.capacity(),
                pos: 0,
            };
            let remaining = parse_code(unsafe {
                zstd_sys::ZSTD_endStream(self.context.s,
                                   &mut buffer as *mut zstd_sys::ZSTD_outBuffer)
            })?;
            unsafe {
                self.buffer.set_len(buffer.pos);
            }
            if remaining != 0 {
                // Need to flush?
                panic!("Need to flush, but I'm lazy.");
            }
            self.offset = 0;
            self.state = EncoderState::StreamEnd;
        }

        self.write_from_offset()?;

        Ok(())
    }

    /// Return a recommendation for the size of data to write at once.
    pub fn recommended_input_size() -> usize {
        unsafe { zstd_sys::ZSTD_CStreamInSize() }
    }

    /// write_all, except keep track of partial writes for non-blocking IO.
    fn write_from_offset(&mut self) -> io::Result<()> {
        while self.offset < self.buffer.len() {
            match self.writer.write(&self.buffer[self.offset..]) {
                Ok(n) => self.offset += n,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => (),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

impl<W: Write> Write for Encoder<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.state != EncoderState::Accepting {
            panic!("write called after try_finish attempted");
        }

        loop {
            // Write any data pending in `self.buffer`.
            self.write_from_offset()?;

            // If we get to here, `self.buffer` can safely be discarded.

            let mut in_buffer = zstd_sys::ZSTD_inBuffer {
                src: buf.as_ptr() as *const c_void,
                size: buf.len(),
                pos: 0,
            };

            let mut out_buffer = zstd_sys::ZSTD_outBuffer {
                dst: self.buffer.as_mut_ptr() as *mut c_void,
                size: self.buffer.capacity(),
                pos: 0,
            };

            unsafe {
                // Time to fill our output buffer
                let code =
                    zstd_sys::ZSTD_compressStream(self.context.s,
                                                  &mut out_buffer as
                                                  *mut zstd_sys::ZSTD_outBuffer,
                                                  &mut in_buffer as
                                                  *mut zstd_sys::ZSTD_inBuffer);
                // Note: this may very well be empty,
                // if it doesn't exceed zstd's own buffer
                self.buffer.set_len(out_buffer.pos);

                // Do we care about the hint?
                let _ = parse_code(code)?;
            }
            self.offset = 0;

            if in_buffer.pos > 0 || buf.len() == 0 {
                return Ok(in_buffer.pos);
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.state == EncoderState::Accepting {
            self.write_from_offset()?;

            let mut buffer = zstd_sys::ZSTD_outBuffer {
                dst: self.buffer.as_mut_ptr() as *mut c_void,
                size: self.buffer.capacity(),
                pos: 0,
            };
            unsafe {
                let code =
                    zstd_sys::ZSTD_flushStream(self.context.s,
                                               &mut buffer as
                                               *mut zstd_sys::ZSTD_outBuffer);
                self.buffer.set_len(buffer.pos);
                let _ = parse_code(code)?;
            }
            self.offset = 0;
        }

        self.write_from_offset()?;
        Ok(())
    }
}

#[cfg(feature = "tokio")]
impl <W: AsyncWrite> AsyncWrite for Encoder<W> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        try_nb!(self.do_finish());
        self.writer.shutdown()
    }
}

#[cfg(test)]
mod tests {
    use super::Encoder;
    use stream::decode_all;
    use stream::tests::WritePartial;

    /// Test that flush after a partial write works successfully without
    /// corrupting the frame. This test is in this module because it checks
    /// internal implementation details.
    #[test]
    fn test_partial_write_flush() {
        use std::io::Write;

        let (input, mut z) = setup_partial_write();

        // flush shouldn't corrupt the stream
        z.flush().unwrap();

        let buf = z.finish().unwrap().into_inner();
        assert_eq!(&decode_all(&buf[..]).unwrap(), &input);
    }

    /// Test that finish after a partial write works successfully without
    /// corrupting the frame. This test is in this module because it checks
    /// internal implementation details.
    #[test]
    fn test_partial_write_finish() {
        let (input, z) = setup_partial_write();

        // finish shouldn't corrupt the stream
        let buf = z.finish().unwrap().into_inner();
        assert_eq!(&decode_all(&buf[..]).unwrap(), &input);
    }

    fn setup_partial_write() -> (Vec<u8>, Encoder<WritePartial>) {
        use std::io::Write;

        let mut buf = WritePartial::new();
        buf.accept(Some(1));
        let mut z = Encoder::new(buf, 1).unwrap();

        // Fill in enough data to make sure the buffer gets written out.
        let input = "b".repeat(128 * 1024).into_bytes();
        z.write(&input).unwrap();

        // At this point, the internal buffer in z should have some data.
        assert_ne!(z.offset, z.buffer.len());

        (input, z)
    }
}

#[cfg(test)]
#[cfg(feature = "tokio")]
mod async_tests {
    use std::io::{self, Cursor};
    use tokio_io::{AsyncWrite, AsyncRead, io as tokio_io};
    use futures::{Future, Poll, task, executor};

    struct BlockingWriter<T: AsyncWrite> {
        block: bool,
        writer: T,
    }

    impl<T: AsyncWrite> BlockingWriter<T> {
        fn new(writer: T)-> BlockingWriter<T> {
            BlockingWriter { block: false, writer: writer }
        }

        fn into_inner(self)-> T {
            self.writer
        }
    }

    impl<T: AsyncWrite> AsyncWrite for BlockingWriter<T> {
        fn shutdown(&mut self) -> Poll<(), io::Error> {
            self.writer.shutdown()
        }
    }

    impl<T: AsyncWrite> io::Write for BlockingWriter<T> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.block {
                self.block = false;
                task::park().unpark();
                Err(io::Error::from(io::ErrorKind::WouldBlock))
            }
            else {
                self.block = true;
                self.writer.write(buf)
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_async_write() {
        use stream::decode_all;

        let source = "abc".repeat(1024 * 100).into_bytes();
        let encoded_output = test_async_write_worker(&source[..], Cursor::new(Vec::new()), |w| { w.into_inner() });
        let decoded = decode_all(&encoded_output[..]).unwrap();
        assert_eq!(source, &decoded[..]);
    }

    #[test]
    fn test_async_write_block() {
        use stream::decode_all;

        let source = "abc".repeat(1024 * 100).into_bytes();
        let encoded_output = test_async_write_worker(&source[..], BlockingWriter::new(Cursor::new(Vec::new())), |w| { w.into_inner().into_inner() });
        let decoded = decode_all(&encoded_output[..]).unwrap();
        assert_eq!(source, &decoded[..]);
    }

    struct Finish<W: AsyncWrite> {
        encoder: Option<super::Encoder<W>>,
    }

    impl<W: AsyncWrite> Future for Finish<W> {
        type Item = W;
        type Error = io::Error;

        fn poll(&mut self) -> Poll<W, io::Error> {
            use futures::Async;

            match self.encoder.take().unwrap().try_finish() {
                Ok(v) => {
                    return Ok(v.into())
                },
                Err((encoder, err)) => {
                    if err.kind() == io::ErrorKind::WouldBlock {
                        self.encoder = Some(encoder);
                        return Ok(Async::NotReady);
                    }
                    else {
                        return Err(err)
                    }
                },
            };
        }
    }

    fn test_async_write_worker<R: AsyncRead, W: AsyncWrite, Res, F: FnOnce(W) -> Res>(r: R, w: W, f: F) -> Res {
        use super::Encoder;

        let encoder = Encoder::new(w, 1).unwrap();
        let copy_future =
            tokio_io::copy(r, encoder)
            .and_then(| (_, _, encoder) | { 
                Finish { encoder: Some (encoder) } 
            })
            .map(f);
        executor::spawn(copy_future).wait_future().unwrap()
    }
}