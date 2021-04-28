use crate::map_error_code;

#[cfg(feature = "experimental")]
use std::convert::TryInto;
use std::io;
use zstd_safe;

/// Allows to decompress independently multiple blocks of data.
///
/// This reduces memory usage compared to calling `decompress` multiple times.
#[derive(Default)]
pub struct Decompressor {
    context: zstd_safe::DCtx<'static>,
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
            context: zstd_safe::create_dctx(),
            dict,
        }
    }

    /// Deompress a single block of data to the given destination buffer.
    ///
    /// Returns the number of bytes written, or an error if something happened
    /// (for instance if the destination buffer was too small).
    pub fn decompress_to_buffer<C: zstd_safe::WriteBuf + ?Sized>(
        &mut self,
        source: &[u8],
        destination: &mut C,
    ) -> io::Result<usize> {
        self.context
            .decompress_using_dict(destination, source, &self.dict)
            .map_err(map_error_code)
    }

    /// Decompress a block of data, and return the result in a `Vec<u8>`.
    ///
    /// The decompressed data should be less than `capacity` bytes,
    /// or an error will be returned.
    pub fn decompress(
        &mut self,
        data: &[u8],
        capacity: usize,
    ) -> io::Result<Vec<u8>> {
        let capacity =
            Self::upper_bound(data).unwrap_or(capacity).min(capacity);
        let mut buffer = Vec::with_capacity(capacity);
        self.decompress_to_buffer(data, &mut buffer)?;
        Ok(buffer)
    }

    /// Get an upper bound on the decompressed size of data, if available
    ///
    /// This can be used to pre-allocate enough capacity for `decompress_to_buffer`
    /// and is used by `decompress` to ensure that it does not over-allocate if
    /// you supply a large `capacity`.
    ///
    /// Will return `None` if the upper bound cannot be determined or is larger than `usize::MAX`
    pub fn upper_bound(_data: &[u8]) -> Option<usize> {
        #[cfg(feature = "experimental")]
        {
            let bound = zstd_safe::decompress_bound(_data).ok()?;
            bound.try_into().ok()
        }
        #[cfg(not(feature = "experimental"))]
        {
            None
        }
    }
}

fn _assert_traits() {
    fn _assert_send<T: Send>(_: T) {}

    _assert_send(Decompressor::new());
}
