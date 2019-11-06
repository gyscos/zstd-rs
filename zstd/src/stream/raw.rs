//! Raw in-memory stream compression/decompression.
//!
//! This module defines a `Decoder` and an `Encoder` to decode/encode streams
//! of data using buffers.
//!
//! They are mostly thin wrappers around `zstd_safe::{DCtx, CCtx}`.
use std::io;

pub use zstd_safe::{CParameter, DParameter, InBuffer, OutBuffer};

use crate::dict::{DecoderDictionary, EncoderDictionary};
use crate::map_error_code;

/// Represents an abstract compression/decompression operation.
///
/// This trait covers both `Encoder` and `Decoder`.
pub trait Operation {
    /// Performs a single step of this operation.
    ///
    /// Should return a hint for the next input size.
    ///
    /// If the result is `Ok(0)`, it may indicate that a frame was just
    /// finished.
    fn run(
        &mut self,
        input: &mut InBuffer<'_>,
        output: &mut OutBuffer<'_>,
    ) -> io::Result<usize>;

    /// Performs a single step of this operation.
    ///
    /// This is a comvenience wrapper around `Operation::run` if you don't
    /// want to deal with `InBuffer`/`OutBuffer`.
    fn run_on_buffers(
        &mut self,
        input: &[u8],
        output: &mut [u8],
    ) -> io::Result<Status> {
        let mut input = InBuffer::around(input);
        let mut output = OutBuffer::around(output);

        let remaining = self.run(&mut input, &mut output)?;

        Ok(Status {
            remaining,
            bytes_read: input.pos,
            bytes_written: output.pos,
        })
    }

    /// Flushes any internal buffer, if any.
    ///
    /// Returns the number of bytes still in the buffer.
    /// To flush entirely, keep calling until it returns `Ok(0)`.
    fn flush(&mut self, output: &mut OutBuffer<'_>) -> io::Result<usize> {
        let _ = output;
        Ok(0)
    }

    /// Prepares the operation for a new frame.
    ///
    /// This is hopefully cheaper than creating a new operation.
    fn reinit(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// Finishes the operation, writing any footer if necessary.
    ///
    /// Returns the number of bytes still to write.
    ///
    /// Keep calling this method until it returns `Ok(0)`,
    /// and then don't ever call this method.
    fn finish(
        &mut self,
        output: &mut OutBuffer<'_>,
        finished_frame: bool,
    ) -> io::Result<usize> {
        let _ = output;
        let _ = finished_frame;
        Ok(0)
    }
}

/// Dummy operation that just copies its input to the output.
pub struct NoOp;

impl Operation for NoOp {
    fn run(
        &mut self,
        input: &mut InBuffer<'_>,
        output: &mut OutBuffer<'_>,
    ) -> io::Result<usize> {
        let src = &input.src[input.pos..];
        let dst = &mut output.dst[output.pos..];

        let len = usize::min(src.len(), dst.len());
        let src = &src[..len];
        let dst = &mut dst[..len];

        dst.copy_from_slice(src);
        input.pos += len;
        output.pos += len;

        Ok(0)
    }
}

/// Describes the result of an operation.
pub struct Status {
    /// Number of bytes expected for next input.
    ///
    /// This is just a hint.
    pub remaining: usize,

    /// Number of bytes read from the input.
    pub bytes_read: usize,

    /// Number of bytes written to the output.
    pub bytes_written: usize,
}

/// An in-memory decoder for streams of data.
pub struct Decoder {
    context: zstd_safe::DCtx<'static>,
}

impl Decoder {
    /// Creates a new decoder.
    pub fn new() -> io::Result<Self> {
        Self::with_dictionary(&[])
    }

    /// Creates a new decoder initialized with the given dictionary.
    pub fn with_dictionary(dictionary: &[u8]) -> io::Result<Self> {
        let mut context = zstd_safe::create_dstream();
        zstd_safe::init_dstream_using_dict(&mut context, dictionary)
            .map_err(map_error_code)?;
        Ok(Decoder { context })
    }

    /// Creates a new decoder, using an existing `DecoderDictionary`.
    pub fn with_prepared_dictionary(
        dictionary: &DecoderDictionary<'_>,
    ) -> io::Result<Self> {
        let mut context = zstd_safe::create_dstream();
        zstd_safe::init_dstream_using_ddict(
            &mut context,
            dictionary.as_ddict(),
        )
        .map_err(map_error_code)?;
        Ok(Decoder { context })
    }

    /// Sets a decompression parameter for this decoder.
    pub fn set_parameter(&mut self, parameter: DParameter) -> io::Result<()> {
        zstd_safe::dctx_set_parameter(&mut self.context, parameter)
            .map_err(map_error_code)?;
        Ok(())
    }
}

impl Operation for Decoder {
    fn run(
        &mut self,
        input: &mut InBuffer<'_>,
        output: &mut OutBuffer<'_>,
    ) -> io::Result<usize> {
        zstd_safe::decompress_stream(&mut self.context, output, input)
            .map_err(map_error_code)
    }

    fn reinit(&mut self) -> io::Result<()> {
        zstd_safe::reset_dstream(&mut self.context).map_err(map_error_code)?;
        Ok(())
    }
    fn finish(
        &mut self,
        _output: &mut OutBuffer<'_>,
        finished_frame: bool,
    ) -> io::Result<usize> {
        if finished_frame {
            Ok(0)
        } else {
            Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete frame",
            ))
        }
    }
}

/// An in-memory encoder for streams of data.
pub struct Encoder {
    context: zstd_safe::CCtx<'static>,
}

impl Encoder {
    /// Creates a new encoder.
    pub fn new(level: i32) -> io::Result<Self> {
        Self::with_dictionary(level, &[])
    }

    /// Creates a new encoder initialized with the given dictionary.
    pub fn with_dictionary(level: i32, dictionary: &[u8]) -> io::Result<Self> {
        let mut context = zstd_safe::create_cstream();
        zstd_safe::init_cstream_using_dict(&mut context, dictionary, level)
            .map_err(map_error_code)?;
        Ok(Encoder { context })
    }

    /// Creates a new encoder using an existing `EncoderDictionary`.
    pub fn with_prepared_dictionary(
        dictionary: &EncoderDictionary<'_>,
    ) -> io::Result<Self> {
        let mut context = zstd_safe::create_cstream();
        zstd_safe::init_cstream_using_cdict(
            &mut context,
            dictionary.as_cdict(),
        )
        .map_err(map_error_code)?;
        Ok(Encoder { context })
    }

    /// Sets a compression parameter for this encoder.
    pub fn set_parameter(&mut self, parameter: CParameter) -> io::Result<()> {
        zstd_safe::cctx_set_parameter(&mut self.context, parameter)
            .map_err(map_error_code)?;
        Ok(())
    }
}

impl Operation for Encoder {
    fn run(
        &mut self,
        input: &mut InBuffer<'_>,
        output: &mut OutBuffer<'_>,
    ) -> io::Result<usize> {
        zstd_safe::compress_stream(&mut self.context, output, input)
            .map_err(map_error_code)
    }

    fn flush(&mut self, output: &mut OutBuffer<'_>) -> io::Result<usize> {
        zstd_safe::flush_stream(&mut self.context, output)
            .map_err(map_error_code)
    }

    fn finish(
        &mut self,
        output: &mut OutBuffer<'_>,
        _finished_frame: bool,
    ) -> io::Result<usize> {
        zstd_safe::end_stream(&mut self.context, output)
            .map_err(map_error_code)
    }

    fn reinit(&mut self) -> io::Result<()> {
        zstd_safe::reset_cstream(
            &mut self.context,
            zstd_safe::CONTENTSIZE_UNKNOWN,
        )
        .map_err(map_error_code)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Decoder, Encoder, InBuffer, Operation, OutBuffer};

    #[test]
    fn test_cycle() {
        let mut encoder = Encoder::new(1).unwrap();
        let mut decoder = Decoder::new().unwrap();

        // Step 1: compress
        let mut input = InBuffer::around(b"AbcdefAbcdefabcdef");

        let mut output = [0u8; 128];
        let mut output = OutBuffer::around(&mut output);

        loop {
            encoder.run(&mut input, &mut output).unwrap();

            if input.pos == input.src.len() {
                break;
            }
        }
        encoder.finish(&mut output, true).unwrap();

        let initial_data = input.src;

        // Step 2: decompress

        let mut input = InBuffer::around(output.as_slice());
        let mut output = [0u8; 128];
        let mut output = OutBuffer::around(&mut output);

        loop {
            decoder.run(&mut input, &mut output).unwrap();

            if input.pos == input.src.len() {
                break;
            }
        }

        assert_eq!(initial_data, output.as_slice());
    }
}
