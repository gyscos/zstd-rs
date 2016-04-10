//! Train a dictionary from various sources.
//!
//! A dictionary can help improve the compression of small files.
//! The dictionary must be present during decompression,
//! but can be shared accross multiple "similar" files.
//!
//! Creating a dictionary using the `zstd` C library,
//! using the `zstd` command-line interface, using this library,
//! or using the `train` binary provided, should give the same result,
//! and are therefore completely compatible.
//!
//! To use, see [`Encoder::with_dictionary`] or [`Decoder::with_dictionary`].
//!
//! [`Encoder::with_dictionary`]: ../struct.Encoder.html#method.with_dictionary
//! [`Decoder::with_dictionary`]: ../struct.Decoder.html#method.with_dictionary

use std::io;
use ll;

/// Train a dictionary from a big continuous chunk of data.
///
/// This is the most efficient way to train a dictionary,
/// since this is directly fed into `zstd`.
pub fn from_continuous(sample_data: &[u8], sample_sizes: &[usize],
                       max_size: usize)
                       -> io::Result<Vec<u8>> {
    // Complain if the lengths don't add up to the entire data.
    if sample_sizes.iter().fold(0, |a, b| a + b) != sample_data.len() {
        return Err(io::Error::new(io::ErrorKind::Other,
                                  "sample sizes don't add up".to_string()));
    }

    let mut result = Vec::with_capacity(max_size);
    unsafe {
        let code = ll::ZDICT_trainFromBuffer(result.as_mut_ptr(),
                                             result.capacity(),
                                             sample_data.as_ptr(),
                                             sample_sizes.as_ptr(),
                                             sample_sizes.len());
        let written = try!(ll::parse_code(code));
        result.set_len(written);
    }
    Ok(result)
}

/// Train a dictionary from multiple samples.
///
/// The samples will internaly be copied to a single continuous buffer,
/// so make sure you have enough memory available.
pub fn from_samples<S: AsRef<[u8]>>(samples: &[S], max_size: usize)
                                    -> io::Result<Vec<u8>> {
    // Copy every sample to a big chunk of memory
    let data: Vec<_> = samples.iter()
                              .flat_map(|s| s.as_ref())
                              .map(|&b| b)
                              .collect();
    let sizes: Vec<_> = samples.iter().map(|s| s.as_ref().len()).collect();

    from_continuous(&data, &sizes, max_size)
}
