use crate::map_error_code;

use std::convert::TryInto;
use std::io;
use zstd_safe;

/// Allows to decompress independently multiple blocks of data.
///
/// This reduces memory usage compared to calling `decompress` multiple times.
#[derive(Default)]
pub struct Decompressor<'a> {
    context: zstd_safe::DCtx<'a>,
}

impl Decompressor<'static> {
    /// Creates a new zstd decompressor.
    pub fn new() -> io::Result<Self> {
        Self::with_dictionary(&[])
    }

    /// Creates a new zstd decompressor, using the given dictionary.
    pub fn with_dictionary(dictionary: &[u8]) -> io::Result<Self> {
        let mut decompressor = Self::default();

        decompressor.set_dictionary(dictionary)?;

        Ok(decompressor)
    }
}

impl<'a> Decompressor<'a> {
    /// Creates a new decompressor using an existing `DecoderDictionary`.
    ///
    /// Note that using a dictionary means that compression will need to use
    /// the same dictionary.
    pub fn with_prepared_dictionary<'b>(
        dictionary: &'a crate::dict::DecoderDictionary<'b>,
    ) -> io::Result<Self>
    where
        'b: 'a,
    {
        let mut decompressor = Self::default();

        decompressor.set_prepared_dictionary(dictionary)?;

        Ok(decompressor)
    }

    /// Changes the dictionary used by this decompressor.
    ///
    /// Will affect future compression jobs.
    ///
    /// Note that using a dictionary means that compression will need to use
    /// the same dictionary.
    pub fn set_dictionary(&mut self, dictionary: &[u8]) -> io::Result<()> {
        self.context
            .load_dictionary(dictionary)
            .map_err(map_error_code)?;

        Ok(())
    }

    /// Changes the dictionary used by this decompressor.
    ///
    /// Note that using a dictionary means that compression will need to use
    /// the same dictionary.
    pub fn set_prepared_dictionary<'b>(
        &mut self,
        dictionary: &'a crate::dict::DecoderDictionary<'b>,
    ) -> io::Result<()>
    where
        'b: 'a,
    {
        self.context
            .ref_ddict(dictionary.as_ddict())
            .map_err(map_error_code)?;

        Ok(())
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
            .decompress(destination, source)
            .map_err(map_error_code)
    }

    /// Decompress a block of data, and return the result in a `Vec<u8>`.
    ///
    /// The decompressed data should be at most `capacity` bytes,
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

    /// Sets a decompression parameter for this decompressor.
    pub fn set_parameter(
        &mut self,
        parameter: zstd_safe::DParameter,
    ) -> io::Result<()> {
        self.context
            .set_parameter(parameter)
            .map_err(map_error_code)?;
        Ok(())
    }

    crate::decoder_parameters!();

    /// Get an upper bound on the decompressed size of data, if available
    ///
    /// This can be used to pre-allocate enough capacity for `decompress_to_buffer`
    /// and is used by `decompress` to ensure that it does not over-allocate if
    /// you supply a large `capacity`.
    ///
    /// Returns `None` if the size cannot be determined, or does not fit a
    /// `usize`. That happens when the frame does not record its decompressed
    /// size - which is normal for anything a streaming compressor produced
    /// without being told the length up front.
    ///
    /// With the `experimental` feature this sums every frame in `data`.
    /// Without it, the answer is limited to a single frame, since the function
    /// that adds them up is part of zstd's experimental API.
    pub fn upper_bound(data: &[u8]) -> Option<usize> {
        // Walking the frames ourselves gives the exact total, and needs
        // nothing experimental.
        if let Some(size) = total_decompressed_size(data) {
            return size.try_into().ok();
        }

        // That only works while every frame records its size. When one does
        // not, zstd can still bound it from the window size, but only through
        // the experimental API.
        #[cfg(feature = "experimental")]
        if let Ok(bound) = zstd_safe::decompress_bound(data) {
            return bound.try_into().ok();
        }

        None
    }
}

/// Adds up the decompressed size of every frame in `data`.
///
/// This is what `ZSTD_findDecompressedSize()` does, except that one is part of
/// zstd's experimental API - and we have the whole input in hand anyway, so
/// walking it is a few lines.
///
/// Returns `None` if `data` is not a clean sequence of frames, or if any of
/// them does not record its decompressed size.
///
/// Skippable frames are handled: zstd reports their full length and a content
/// size of zero, so they are stepped over without affecting the total.
fn total_decompressed_size(mut data: &[u8]) -> Option<u64> {
    let mut total = 0u64;

    while !data.is_empty() {
        let frame_len = zstd_safe::find_frame_compressed_size(data).ok()?;
        let content = zstd_safe::get_frame_content_size(data).ok()??;

        total = total.checked_add(content)?;
        data = data.get(frame_len..)?;
    }

    Some(total)
}

fn _assert_traits() {
    fn _assert_send<T: Send>(_: T) {}

    _assert_send(Decompressor::new());
}
