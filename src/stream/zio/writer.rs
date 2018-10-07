use std::io::{self, Write};

use stream::raw::Operation;

use zstd_safe;

// input -> [ zstd -> buffer -> writer ]

/// Implements the [`std::io::Writer`] API around an operation.
///
/// This can be used to wrap a raw in-memory operation in a write-focused API.
///
/// It can be used with either compression or decompression, and forwards the
/// output to a wrapped `Write`.
pub struct Writer<W, D> {
    writer: W,
    operation: D,

    offset: usize,
    buffer: Vec<u8>,

    // When `true`, indicates that nothing should be added to the buffer.
    // All that's left if to empty the buffer.
    finished: bool,
}

impl<W, D> Writer<W, D>
where
    W: Write,
    D: Operation,
{
    /// Creates a new `Writer`.
    ///
    /// All output from the given operation will be forwarded to `writer`.
    pub fn new(writer: W, operation: D) -> Self {
        Writer {
            writer,
            operation,

            offset: 0,
            // 32KB buffer? That's what flate2 uses
            buffer: Vec::with_capacity(32 * 1024),

            finished: false,
        }
    }

    /// Ends the stream.
    ///
    /// This *must* be called after all data has been written to finish the
    /// stream.
    ///
    /// If you forget to call this and just drop the `Writer`, you *will* have
    /// an incomplete output.
    ///
    /// If this method returns `Interrupted`, keep calling it until it returns
    /// `Ok(())`, then don't call it again.
    pub fn finish(&mut self) -> io::Result<()> {
        loop {
            // Keep trying until we're really done.
            self.write_from_offset()?;

            // At this point the buffer has been fully written out.

            if self.finished {
                return Ok(());
            }

            // Let's fill this buffer again!

            let (bytes_written, hint) = {
                unsafe { self.expand_buffer() };
                let mut output =
                    zstd_safe::OutBuffer::around(&mut self.buffer);
                let hint = self.operation.finish(&mut output);

                (output.pos, hint)
            };
            // We return here if zstd had a problem.
            // Could happen with invalid data, ...
            let hint = hint?;

            self.offset = 0;
            unsafe { self.buffer.set_len(bytes_written) };

            self.finished = hint == 0;
        }
    }

    /// Expands the buffer before writing there.
    ///
    /// This will leave the buffer with potentially uninitialized memory.
    unsafe fn expand_buffer(&mut self) {
        let capacity = self.buffer.capacity();
        self.buffer.set_len(capacity);
    }

    /// Attempt to write `self.buffer` to the wrapped writer.
    ///
    /// Returns `Ok(())` once all the buffer has been written.
    fn write_from_offset(&mut self) -> io::Result<()> {
        // The code looks a lot like `write_all`, but keeps track of what has
        // been written in case we're interrupted.
        while self.offset < self.buffer.len() {
            match self.writer.write(&self.buffer[self.offset..]) {
                Ok(n) => self.offset += n,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => (),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// Return the wrapped `Writer` and `Operation`.
    ///
    /// Careful: if you call this before calling [`Self::finish()`], the
    /// output may be incomplete.
    pub fn into_inner(self) -> (W, D) {
        (self.writer, self.operation)
    }
}

impl<W, D> Write for Writer<W, D>
where
    W: Write,
    D: Operation,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Keep trying until _something_ has been consumed.
        // As soon as some input has been taken, we cannot afford
        // to take any chance: if an error occurs, the user couldn't know
        // that some data _was_ successfully written.
        loop {
            // First, write any pending data from `self.buffer`.
            self.write_from_offset()?;

            // At this point `self.buffer` can safely be discarded.
            let (bytes_read, bytes_written, hint) = {
                unsafe { self.expand_buffer() };
                let mut src = zstd_safe::InBuffer::around(buf);
                let mut dst = zstd_safe::OutBuffer::around(&mut self.buffer);

                let hint = self.operation.run(&mut src, &mut dst);

                (src.pos, dst.pos, hint)
            };

            // println!("Read {}; Wrote {}", bytes_read, bytes_written);

            self.offset = 0;
            unsafe { self.buffer.set_len(bytes_written) };
            let _ = hint?;

            // As we said, as soon as we've consumed something, return.
            if bytes_read > 0 || buf.is_empty() {
                return Ok(bytes_read);
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut finished = false;
        loop {
            // If the output is blocked or has an error, return now.
            self.write_from_offset()?;

            if finished {
                return Ok(());
            }

            let (bytes_written, hint) = {
                unsafe { self.expand_buffer() };
                let mut dst = zstd_safe::OutBuffer::around(&mut self.buffer);
                let hint = self.operation.flush(&mut dst);

                (dst.pos, hint)
            };

            let hint = hint?;
            self.offset = 0;
            unsafe { self.buffer.set_len(bytes_written) };

            finished = hint == 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Writer;
    use std::io::{Cursor, Read, Write};

    #[test]
    fn test_noop() {
        use stream::raw::NoOp;

        let input = b"AbcdefghAbcdefgh.";

        // Test writer
        let mut output = Vec::new();
        {
            let mut writer = Writer::new(&mut output, NoOp);
            writer.write_all(input).unwrap();
            writer.finish().unwrap();
        }
        assert_eq!(&output, input);
    }

    #[test]
    fn test_compress() {
        use stream::raw::Encoder;

        let input = b"AbcdefghAbcdefgh.";

        // Test writer
        let mut output = Vec::new();
        {
            let mut writer =
                Writer::new(&mut output, Encoder::new(1).unwrap());
            writer.write_all(input).unwrap();
            writer.finish().unwrap();
        }
        // println!("Output: {:?}", output);
        let decoded = ::decode_all(&output[..]).unwrap();
        assert_eq!(&decoded, input);
    }

    #[test]
    fn test_decompress() {
        use stream::raw::Decoder;

        let input = b"AbcdefghAbcdefgh.";
        let compressed = ::encode_all(&input[..], 1).unwrap();

        // Test writer
        let mut output = Vec::new();
        {
            let mut writer = Writer::new(&mut output, Decoder::new().unwrap());
            writer.write_all(&compressed).unwrap();
            writer.finish().unwrap();
        }
        // println!("Output: {:?}", output);
        assert_eq!(&output, input);
    }
}
