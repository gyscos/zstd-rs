use ll;
use ::parse_code;

use std::io;

struct EncoderContext {
    c: *mut ll::ZSTD_CCtx,
}

impl Default for EncoderContext {
    fn default() -> Self {
        EncoderContext { c: unsafe { ll::ZSTD_createCCtx() } }
    }
}

impl Drop for EncoderContext {
    fn drop(&mut self) {
        let code = unsafe { ll::ZSTD_freeCCtx(self.c) };
        parse_code(code).unwrap();
    }
}

/// Allows to compress independently multiple blocks of data.
///
/// This reduces memory usage compared to calling `compress` multiple times.
/// The compressed blocks are still completely independent.
#[derive(Default)]
pub struct Compressor {
    context: EncoderContext,
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
            context: EncoderContext::default(),
            dict: dict,
        }
    }

    /// Compress a single block of data to the given destination buffer.
    ///
    /// Returns the number of bytes written, or an error if something happened
    /// (for instance if the destination buffer was too small).
    pub fn compress_to_buffer(&mut self, source: &[u8],
                              destination: &mut [u8], level: i32)
                              -> io::Result<usize> {
        let code = unsafe {
            ll::ZSTD_compress_usingDict(self.context.c,
                                        destination.as_mut_ptr(),
                                        destination.len(),
                                        source.as_ptr(),
                                        source.len(),
                                        self.dict.as_ptr(),
                                        self.dict.len(),
                                        level)
        };
        parse_code(code)
    }

    /// Compresses a block of data and returns the compressed result.
    pub fn compress(&mut self, data: &[u8], lvl: i32) -> io::Result<Vec<u8>> {
        // We allocate a big buffer, slightly larger than the input data.
        let buffer_len = unsafe { ll::ZSTD_compressBound(data.len()) };
        let mut buffer = Vec::with_capacity(buffer_len);
        unsafe {
            // Use all capacity.
            // Memory may not be initialized, but we won't read it.
            buffer.set_len(buffer_len);
            let len =
                try!(self.compress_to_buffer(data, &mut buffer[..], lvl));
            buffer.set_len(len);
        }

        // Should we shrink the vec? Meh, let the user do it if he wants.
        Ok(buffer)
    }
}
