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

    finished: bool,

    single_frame: bool,
    finished_frame: bool,
}

impl<R, D> Reader<R, D> {
    /// Creates a new `Reader`.
    ///
    /// `reader` will be used to pull input data for the given operation.
    pub fn new(reader: R, operation: D) -> Self {
        Reader {
            reader,
            operation,
            finished: false,
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
    reader.fill_buf()
}

impl<R, D> Read for Reader<R, D>
where
    R: BufRead,
    D: Operation,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.finished {
            return Ok(0);
        }

        // Keep trying until _something_ has been written.
        loop {
            let (bytes_read, bytes_written) = {
                // Start with a fresh pool of un-processed data.
                // This is the only line that can return an interuption error.
                let input = fill_buf(&mut self.reader)?;

                // println!("{:?}", input);

                // It's possible we don't have any new data to read.
                // (In this case we may still have zstd's own buffer to clear.)
                let eof = input.is_empty();

                let mut src = InBuffer::around(input);
                let mut dst = OutBuffer::around(buf);

                // This can only fail with invalid data.
                // The return value is a hint for the next input size,
                // but it's safe to ignore.
                if !eof {
                    if self.finished_frame {
                        self.operation.reinit()?;
                        self.finished_frame = false;
                    }

                    // Phase 1: feed input to the operation
                    let hint = self.operation.run(&mut src, &mut dst)?;

                    if hint == 0 {
                        // We just finished a frame.
                        self.finished_frame = true;
                        if self.single_frame {
                            self.finished = true;
                        }
                    }
                } else {
                    // TODO: Make it Work!

                    // Phase 2: flush out the operation's buffer
                    // Keep calling `finish()` until the buffer is empty.
                    let hint = self
                        .operation
                        .finish(&mut dst, self.finished_frame)?;
                    // println!("Hint: {}\nOutput: {:?}", hint, dst);
                    if hint == 0 {
                        // This indicates that the footer is complete.
                        // This is the only way to terminate the stream cleanly.
                        self.finished = true;
                        if dst.pos() == 0 {
                            return Ok(0);
                        }
                    }
                }

                // println!("{:?}", dst);

                (src.pos(), dst.pos())
            };
            self.reader.consume(bytes_read);

            if bytes_written > 0 {
                return Ok(bytes_written);
            }
            // We need more data! Try again!
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
        // println!("{:?}", output);
        let decoded = crate::decode_all(&output[..]).unwrap();
        assert_eq!(&decoded, input);
    }
}
