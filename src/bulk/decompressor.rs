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
        // When every frame records its size, we know exactly how much this
        // produces, so a smaller capacity cannot work. Say so with both
        // numbers rather than letting zstd report a bare "destination buffer
        // is too small" after trying.
        //
        // It has to be the exact figure: `upper_bound` may hand back an
        // over-estimate for a frame that records nothing, and refusing on
        // that would reject input which decompresses perfectly well.
        let exact = total_decompressed_size(data);

        let capacity = match exact {
            Some(needed) if needed > capacity as u64 => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!(
                        "Destination buffer is too small: {} bytes needed, \
                         {} given",
                        needed, capacity
                    ),
                ))
            }
            // Known to fit, since it is not greater than `capacity`.
            Some(needed) => needed as usize,
            // The walk has already told us there is no total to be had, so
            // go straight to zstd rather than back through `upper_bound`,
            // which would only repeat it.
            None => bound_from_zstd(data).unwrap_or(capacity).min(capacity),
        };

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
        // nothing experimental. Failing that, zstd can bound a frame that
        // records no size, but only through its experimental API.
        total_decompressed_size(data)
            .and_then(|size| size.try_into().ok())
            .or_else(|| bound_from_zstd(data))
    }
}

/// What zstd can say about frames that do not record their decompressed size.
///
/// It bounds each one from its block count and window size, which needs
/// walking the block headers - so this is the one part we do not reimplement,
/// and without the experimental feature there is nothing to offer.
fn bound_from_zstd(_data: &[u8]) -> Option<usize> {
    #[cfg(feature = "experimental")]
    if let Ok(bound) = zstd_safe::decompress_bound(_data) {
        return bound.try_into().ok();
    }

    None
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
