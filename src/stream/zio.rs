use std::io::{self, Write};

use super::raw::Operation;

use zstd_safe;

pub struct Reader<R, D> {
    reader: R,
    operation: D,
    pub needs_data: bool,
}

pub struct Writer<W, D> {
    writer: W,
    operation: D,

    offset: usize,
    buffer: Vec<u8>,
    finished: bool,
}

impl<W, D> Writer<W, D>
where
    W: Write,
    D: Operation,
{
    /// Expands the buffer before writing there.
    unsafe fn expand_buffer(&mut self) {
        let capacity = self.buffer.capacity();
        self.buffer.set_len(capacity);
    }

    /// Attempt to write `self.buffer`
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
        self.write_from_offset()?;

        let (bytes_written, hint) = {
            unsafe { self.expand_buffer() };
            let mut dst = zstd_safe::OutBuffer::around(&mut self.buffer);
            let hint = self.operation.flush(&mut dst);

            (dst.pos, hint)
        };

        self.offset = 0;

        unsafe { self.buffer.set_len(bytes_written) };
        let _ = hint?;

        self.write_from_offset()?;

        Ok(())
    }
}
