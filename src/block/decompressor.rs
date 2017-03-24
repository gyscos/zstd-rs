use libc::c_void;
use parse_code;

use std::io;
use zstd_sys;

struct DecoderContext {
    c: *mut zstd_sys::ZSTD_DCtx,
}

impl Default for DecoderContext {
    fn default() -> Self {
        DecoderContext { c: unsafe { zstd_sys::ZSTD_createDCtx() } }
    }
}

impl Drop for DecoderContext {
    fn drop(&mut self) {
        let code = unsafe { zstd_sys::ZSTD_freeDCtx(self.c) };
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
            let destination_ptr = destination.as_mut_ptr() as *mut c_void;
            let source_ptr = source.as_ptr() as *const c_void;
            let dict_ptr = self.dict.as_ptr() as *const c_void;
            zstd_sys::ZSTD_decompress_usingDict(self.context.c,
                                                destination_ptr,
                                                destination.len(),
                                                source_ptr,
                                                source.len(),
                                                dict_ptr,
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
