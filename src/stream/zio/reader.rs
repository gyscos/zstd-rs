use std::io::{self, BufRead, Read};

use crate::stream::raw::{InBuffer, Operation, OutBuffer};

// [ reader -> zstd ] -> output
/// Implements the [`Read`] API around an [`Operation`].
///
/// This can be used to wrap a raw in-memory operation in a read-focused API.
///
/// It can wrap either a compression or decompression operation, and pulls
/// input data from a wrapped `Read`.
pub struct Reader<R, D> {
    reader: R,
    operation: D,

    state: State,

    single_frame: bool,
    finished_frame: bool,
}

enum State {
    // Still actively reading from the inner `Read`
    Reading,
    // We reached EOF from the inner `Read`, now flushing.
    PastEof,
    // We are fully done, nothing can be read.
    Finished,
}

impl<R, D> Reader<R, D> {
    /// Creates a new `Reader`.
    ///
    /// `reader` will be used to pull input data for the given operation.
    pub fn new(reader: R, operation: D) -> Self {
        Reader {
            reader,
            operation,
            state: State::Reading,
            single_frame: false,
            finished_frame: false,
        }
    }

    /// Sets `self` to stop after the first decoded frame.
    pub fn set_single_frame(&mut self) {
        self.single_frame = true;
    }

    /// Returns a mutable reference to the underlying operation.
    pub fn operation_mut(&mut self) -> &mut D {
        &mut self.operation
    }

    /// Returns a mutable reference to the underlying reader.
    pub fn reader_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Returns a reference to the underlying reader.
    pub fn reader(&self) -> &R {
        &self.reader
    }

    /// Returns the inner reader.
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Flush any internal buffer.
    ///
    /// For encoders, this ensures all input consumed so far is compressed.
    pub fn flush(&mut self, output: &mut [u8]) -> io::Result<usize>
    where
        D: Operation,
    {
        self.operation.flush(&mut OutBuffer::around(output))
    }
}

impl<R, D> Reader<R, D>
where
    R: BufRead,
    D: Operation,
{
    /// Consume the rest of the current frame from the underlying reader.
    ///
    /// Once all the decoded data has been read, zstd may still not have
    /// consumed the tail of the frame: it can produce the last of the output
    /// before reading the frame epilogue. The underlying reader is then left
    /// pointing somewhere inside the frame rather than just after it.
    ///
    /// This feeds zstd the input it still needs, with no room for output, so
    /// the reader ends up positioned exactly at the end of the frame.
    ///
    /// It stops there: it will not start decoding whatever follows, so a
    /// stream of concatenated frames keeps its remaining frames, and trailing
    /// non-zstd data is left untouched.
    ///
    /// This is a no-op if the current frame is already complete, so it will
    /// not read from the underlying reader in that case.
    pub fn finish_frame(&mut self) -> io::Result<()> {
        // Only pull on the reader if zstd is actually waiting for the rest of
        // a frame. Otherwise this could block on a stream - a socket, say -
        // that has nothing more to give.
        if self.finished_frame || !matches!(self.state, State::Reading) {
            return Ok(());
        }

        loop {
            let bytes_read = {
                let input = fill_buf(&mut self.reader)?;
                if input.is_empty() {
                    return Ok(());
                }

                let mut src = InBuffer::around(input);
                // No output space: zstd will only consume the input backing
                // the output it has already handed us, and stops at the end of
                // the frame rather than starting the next one.
                let mut dst = OutBuffer::around(&mut [][..]);

                let hint = self.operation.run(&mut src, &mut dst)?;
                if hint == 0 {
                    self.finished_frame = true;
                }

                src.pos()
            };

            self.reader.consume(bytes_read);

            // Either we reached the end of the frame, or we cannot make any
            // more progress without somewhere to put the output.
            if self.finished_frame || bytes_read == 0 {
                return Ok(());
            }
        }
    }
}
// Read and retry on Interrupted errors.
fn fill_buf<R>(reader: &mut R) -> io::Result<&[u8]>
where
    R: BufRead,
{
    // This doesn't work right now because of the borrow-checker.
    // When it can be made to compile, it would allow Reader to automatically
    // retry on `Interrupted` error.
    /*
    loop {
        match reader.fill_buf() {
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
            otherwise => return otherwise,
        }
    }
    */

    // Workaround for now
    let res = reader.fill_buf()?;

    // eprintln!("Filled buffer: {:?}", res);

    Ok(res)
}

impl<R, D> Read for Reader<R, D>
where
    R: BufRead,
    D: Operation,
{
    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
        let start = buf.len();
        let mut written = start;
        loop {
            let result = if written == buf.capacity() {
                // Check for EOF before growing a full (or empty) vector.
                let mut probe = [0; 32];
                self.read(&mut probe).map(|n| {
                    buf.extend_from_slice(&probe[..n]);
                    n
                })
            } else {
                // Initialize only one block of spare capacity at a time.
                // Keep it initialized across short reads so we don't repeatedly
                // zero the same space. Bounded slices retain streaming behavior.
                if written == buf.len() {
                    let chunk = (buf.capacity() - written)
                        .min(zstd_safe::BLOCKSIZE_MAX as usize);
                    buf.resize(written + chunk, 0);
                }
                self.read(&mut buf[written..])
            };
            match result {
                Ok(0) => {
                    buf.truncate(written);
                    return Ok(written - start);
                }
                Ok(n) => written += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => {
                    buf.truncate(written);
                    return Err(e);
                }
            }
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // `Read::read` is specified to return `Ok(0)` for an empty buffer.
        // Without this, the loop below would keep asking the operation to write
        // into no space at all, until zstd gives up with a "no progress" error.
        if buf.is_empty() {
            return Ok(0);
        }

        // Keep trying until _something_ has been written.
        let mut first = true;
        loop {
            match self.state {
                State::Reading => {
                    let (bytes_read, bytes_written) = {
                        // Start with a fresh pool of un-processed data.
                        // This is the only line that can return an interruption error.
                        let input = if first {
                            // eprintln!("First run, no input coming.");
                            b""
                        } else {
                            fill_buf(&mut self.reader)?
                        };

                        // eprintln!("Input = {:?}", input);

                        // It's possible we don't have any new data to read.
                        // (In this case we may still have zstd's own buffer to clear.)
                        if !first && input.is_empty() {
                            self.state = State::PastEof;
                            continue;
                        }
                        first = false;

                        let mut src = InBuffer::around(input);
                        let mut dst = OutBuffer::around(buf);

                        // We don't want empty input (from first=true) to cause a frame
                        // re-initialization.
                        if self.finished_frame && !input.is_empty() {
                            // eprintln!("!! Reigniting !!");
                            self.operation.reinit()?;
                            self.finished_frame = false;
                        }

                        // Phase 1: feed input to the operation
                        let hint = self.operation.run(&mut src, &mut dst)?;
                        // eprintln!(
                        //     "Hint={} Just run our operation:\n In={:?}\n Out={:?}",
                        //     hint, src, dst
                        // );

                        if hint == 0 {
                            // In practice this only happens when decoding, when we just finished
                            // reading a frame.
                            self.finished_frame = true;
                            if self.single_frame {
                                self.state = State::Finished;
                            }
                        }

                        // eprintln!("Output: {:?}", dst);

                        (src.pos(), dst.pos())
                    };

                    self.reader.consume(bytes_read);

                    if bytes_written > 0 {
                        return Ok(bytes_written);
                    }

                    // We need more data! Try again!
                }
                State::PastEof => {
                    let mut dst = OutBuffer::around(buf);

                    // We already sent all the input we could get to zstd. Time to flush out the
                    // buffer and be done with it.

                    // Phase 2: flush out the operation's buffer
                    // Keep calling `finish()` until the buffer is empty.
                    let hint = self
                        .operation
                        .finish(&mut dst, self.finished_frame)?;
                    // eprintln!("Hint: {} ; Output: {:?}", hint, dst);
                    if hint == 0 {
                        // This indicates that the footer is complete.
                        // This is the only way to terminate the stream cleanly.
                        self.state = State::Finished;
                    }

                    return Ok(dst.pos());
                }
                State::Finished => {
                    return Ok(0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Reader;
    use std::io::{Cursor, Read};

    #[test]
    fn test_noop() {
        use crate::stream::raw::NoOp;

        let input = b"AbcdefghAbcdefgh.";

        // Test reader
        let mut output = Vec::new();
        {
            let mut reader = Reader::new(Cursor::new(input), NoOp);
            reader.read_to_end(&mut output).unwrap();
        }
        assert_eq!(&output, input);
    }

    #[test]
    fn test_noop_read_to_end_appends_across_chunks() {
        use crate::stream::raw::NoOp;

        let input = vec![42; zstd_safe::BLOCKSIZE_MAX as usize + 17];
        for spare in [0, 1, input.len(), input.len() + 1] {
            let mut output = Vec::with_capacity(6 + spare);
            output.extend_from_slice(b"prefix");
            let mut reader = Reader::new(&input[..], NoOp);
            assert_eq!(reader.read_to_end(&mut output).unwrap(), input.len());
            assert_eq!(&output[..6], b"prefix");
            assert_eq!(&output[6..], input);
            assert_eq!(reader.read_to_end(&mut output).unwrap(), 0);
        }
    }

    #[test]
    fn test_compress() {
        use crate::stream::raw::Encoder;

        let input = b"AbcdefghAbcdefgh.";

        // Test reader
        let mut output = Vec::new();
        {
            let mut reader =
                Reader::new(Cursor::new(input), Encoder::new(1).unwrap());
            reader.read_to_end(&mut output).unwrap();
        }
        // eprintln!("{:?}", output);
        let decoded = crate::decode_all(&output[..]).unwrap();
        assert_eq!(&decoded, input);
    }
}
