use std::io::{self, BufRead, Read};

use stream::raw::Operation;

use zstd_safe;

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
        }
    }

    /// Returns a mutable reference to the underlying operation.
    pub fn operation_mut(&mut self) -> &mut D {
        &mut self.operation
    }

    /// Returns a mutable reference to the underlying reader.
    pub fn reader_mut(&mut self) -> &mut R {
        &mut self.reader
    }
}

impl<R, D> Read for Reader<R, D>
where
    R: BufRead,
    D: Operation,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Keep trying until _something_ has been written.
        if self.finished {
            return Ok(0);
        }

        loop {
            // Start with a fresh pool of un-processed data.
            // This is the only line that can return an interuption error.
            let (bytes_read, bytes_written, eof) = {
                let input = self.reader.fill_buf()?;

                // println!("{:?}", input);

                // It's possible we don't have any new data to read.
                // (In this case we may have zstd's own buffer to clear.)
                let eof = input.is_empty();

                let mut src = zstd_safe::InBuffer::around(input);
                let mut dst = zstd_safe::OutBuffer::around(buf);

                // This can only fail with invalid data.
                // The return value is a hint for the next input size,
                // but it's safe to ignore.
                if !eof {
                    // TODO: if the result is 0, it may mean that we just
                    // finished reading a ZSTD frame.
                    // TODO: re-init the context if we want to read sequential streams?
                    let _ = self.operation.run(&mut src, &mut dst)?;
                } else {
                    let hint = self.operation.finish(&mut dst)?;
                    if hint == 0 {
                        // This indicates that the footer is complete.
                        self.finished = true;
                    } else if dst.pos == 0 {
                        // Didn't output anything? Maybe we have an incomplete frame?
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "incomplete frame",
                        ));
                    }
                }

                // println!("{:?}", dst);

                (src.pos, dst.pos, eof)
            };
            self.reader.consume(bytes_read);

            if bytes_written > 0 || eof {
                return Ok(bytes_written);
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
        use stream::raw::NoOp;

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
        use stream::raw::Encoder;

        let input = b"AbcdefghAbcdefgh.";

        // Test reader
        let mut output = Vec::new();
        {
            let mut reader =
                Reader::new(Cursor::new(input), Encoder::new(1).unwrap());
            reader.read_to_end(&mut output).unwrap();
        }
        // println!("{:?}", output);
        let decoded = ::decode_all(&output[..]).unwrap();
        assert_eq!(&decoded, input);
    }
}
