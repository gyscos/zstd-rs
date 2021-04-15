use crate::map_error_code;

use std::io;
use zstd_safe;

/// Allows to compress independently multiple blocks of data.
///
/// This reduces memory usage compared to calling `compress` multiple times.
/// The compressed blocks are still completely independent.
#[derive(Default)]
pub struct Compressor {
    context: zstd_safe::CCtx<'static>,
    dict: Vec<u8>,
}

impl Compressor {
    /// Creates a new zstd compressor
    pub fn new() -> Self {
        Compressor::with_dict(Vec::new())
    }

    /// Creates a new zstd compressor, using the given dictionary.
    pub fn with_dict(dict: Vec<u8>) -> Self {
        Compressor {
            context: zstd_safe::create_cctx(),
            dict,
        }
    }

    /// Compress a single block of data to the given destination buffer.
    ///
    /// Returns the number of bytes written, or an error if something happened
    /// (for instance if the destination buffer was too small).
    ///
    /// A level of `0` uses zstd's default (currently `3`).
    pub fn compress_to_buffer<C: zstd_safe::WriteBuf + ?Sized>(
        &mut self,
        source: &[u8],
        destination: &mut C,
        level: i32,
    ) -> io::Result<usize> {
        self.context
            .compress_using_dict(destination, source, &self.dict[..], level)
            .map_err(map_error_code)
    }

    /// Compresses a block of data and returns the compressed result.
    ///
    /// A level of `0` uses zstd's default (currently `3`).
    pub fn compress(
        &mut self,
        data: &[u8],
        level: i32,
    ) -> io::Result<Vec<u8>> {
        // We allocate a big buffer, slightly larger than the input data.
        let buffer_len = zstd_safe::compress_bound(data.len());
        let mut buffer = Vec::with_capacity(buffer_len);

        self.compress_to_buffer(data, &mut buffer, level)?;

        // Should we shrink the vec? Meh, let the user do it if he wants.
        Ok(buffer)
    }
}

fn _assert_traits() {
    fn _assert_send<T: Send>(_: T) {}

    _assert_send(Compressor::new());
}
