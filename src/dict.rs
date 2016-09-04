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

use ll;
use ::parse_code;

use std::io::{self, Read};
use std::path;
use std::fs;

/// Train a dictionary from a big continuous chunk of data.
///
/// This is the most efficient way to train a dictionary,
/// since this is directly fed into `zstd`.
pub fn from_continuous(sample_data: &[u8], sample_sizes: &[usize],
                       max_size: usize)
                       -> io::Result<Vec<u8>> {
    // Complain if the lengths don't add up to the entire data.
    if sample_sizes.iter().sum::<usize>() != sample_data.len() {
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
        let written = try!(parse_code(code));
        result.set_len(written);
    }
    Ok(result)
}

/// Train a dictionary from multiple samples.
///
/// The samples will internaly be copied to a single continuous buffer,
/// so make sure you have enough memory available.
///
/// If you need to stretch your system's limits,
/// [`from_continuous`] directly uses the given slice.
///
/// [`from_continuous`]: ./fn.from_continuous.html
pub fn from_samples<S: AsRef<[u8]>>(samples: &[S], max_size: usize)
                                    -> io::Result<Vec<u8>> {
    // Copy every sample to a big chunk of memory
    let data: Vec<_> = samples.iter()
        .flat_map(|s| s.as_ref())
        .cloned()
        .collect();
    let sizes: Vec<_> = samples.iter().map(|s| s.as_ref().len()).collect();

    from_continuous(&data, &sizes, max_size)
}

/// Train a dict from a list of files.
pub fn from_files<I, P>(filenames: I, max_size: usize) -> io::Result<Vec<u8>>
    where P: AsRef<path::Path>,
          I: IntoIterator<Item = P>
{
    let mut buffer = Vec::new();
    let mut sizes = Vec::new();

    for filename in filenames {
        let mut file = try!(fs::File::open(filename));
        let len = try!(file.read_to_end(&mut buffer));
        sizes.push(len);
    }

    from_continuous(&buffer, &sizes, max_size)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::io::Read;

    #[test]
    fn test_dict_training() {
        // Train a dictionary
        let paths: Vec<_> = fs::read_dir("src")
            .unwrap()
            .map(|entry| entry.unwrap())
            .map(|entry| entry.path())
            .filter(|path| path.to_str().unwrap().ends_with(".rs"))
            .collect();

        let dict = super::from_files(&paths, 4000).unwrap();

        for path in paths {
            let mut buffer = Vec::new();
            let mut file = fs::File::open(path).unwrap();
            let mut content = Vec::new();
            file.read_to_end(&mut content).unwrap();
            io::copy(&mut &content[..],
                     &mut ::stream::Encoder::with_dictionary(&mut buffer,
                                                             1,
                                                             &dict)
                         .unwrap()
                         .auto_finish())
                .unwrap();

            let mut result = Vec::new();
            io::copy(&mut ::stream::Decoder::with_dictionary(&buffer[..],
                                                             &dict[..])
                         .unwrap(),
                     &mut result)
                .unwrap();

            assert_eq!(&content, &result);
        }
    }
}
