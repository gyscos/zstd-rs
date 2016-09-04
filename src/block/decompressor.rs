use ll;
use ::parse_code;

use std::io;

struct DecoderContext {
    c: *mut ll::ZSTD_DCtx,
}

impl Default for DecoderContext {
    fn default() -> Self {
        DecoderContext { c: unsafe { ll::ZSTD_createDCtx() } }
    }
}

impl Drop for DecoderContext {
    fn drop(&mut self) {
        let code = unsafe { ll::ZSTD_freeDCtx(self.c) };
        parse_code(code).unwrap();
    }
}

/// Allows to decompress independently multiple blocks of data.
///
/// This reduces memory usage compared to calling `decompress` multiple times.
#[derive(Default)]
pub struct Decompressor {
    context: DecoderContext,
    dict: Vec<u8>,
}

impl Decompressor {
    /// Creates a new zstd decompressor.
    pub fn new() -> Self {
        Decompressor::with_dict(Vec::new())
    }

    /// Creates a new zstd decompressor, using the given dictionary.
    pub fn with_dict(dict: Vec<u8>) -> Self {
        Decompressor {
            context: DecoderContext::default(),
            dict: dict,
        }
    }

    /// Deompress a single block of data to the given destination buffer.
    ///
    /// Returns the number of bytes written, or an error if something happened
    /// (for instance if the destination buffer was too small).
    pub fn decompress_to_buffer(&mut self, source: &[u8],
                                destination: &mut [u8])
                                -> io::Result<usize> {
        let code = unsafe {
            ll::ZSTD_decompress_usingDict(self.context.c,
                                          destination.as_mut_ptr(),
                                          destination.len(),
                                          source.as_ptr(),
                                          source.len(),
                                          self.dict.as_ptr(),
                                          self.dict.len())
        };
        parse_code(code)
    }

    /// Decompress a block of data, and return the decompressed result in a `Vec<u8>`.
    ///
    /// The decompressed data should be less than `capacity` bytes,
    /// or an error will be returned.
    pub fn decompress(&mut self, data: &[u8], capacity: usize)
                      -> io::Result<Vec<u8>> {
        let mut buffer = Vec::with_capacity(capacity);
        unsafe {
            buffer.set_len(capacity);
            let len = try!(self.decompress_to_buffer(data, &mut buffer[..]));
            buffer.set_len(len);
        }
        Ok(buffer)
    }
}
