//! Compress and decompress Zstd streams.
//!
//! This module provide a `Read`/`Write` interface to zstd streams of arbitrary length.
//!
//! They are compatible with the `zstd` command-line tool.

use std::io;

mod encoder;
mod decoder;

pub use self::encoder::{AutoFinishEncoder, Encoder};
pub use self::decoder::Decoder;

/// Decompress the given data as if using a `Decoder`.
///
/// The input data must be in the zstd frame format.
pub fn decode_all(data: &[u8]) -> io::Result<Vec<u8>> {
    let mut result = Vec::new();
    try!(decode_to_buffer(data, &mut result));
    Ok(result)
}

/// Decompress the given data as if using a `Decoder`.
///
/// Decompressed data will be appended to `destination`.
pub fn decode_to_buffer(source: &[u8], destination: &mut Vec<u8>)
                        -> io::Result<()> {
    let mut decoder = try!(Decoder::new(source));
    try!(io::copy(&mut decoder, destination));
    Ok(())
}

/// Compress all the given data as if using an `Encoder`.
///
/// Result will be in the zstd frame format.
pub fn encode_all(data: &[u8], level: i32) -> io::Result<Vec<u8>> {
    let mut result = Vec::<u8>::new();
    try!(encode_to_buffer(data, &mut result, level));
    Ok(result)
}

/// Compress all the given data as if using an `Encoder`.
///
/// Compressed data will be appended to `destination`.
pub fn encode_to_buffer(data: &[u8], destination: &mut Vec<u8>, level: i32)
                        -> io::Result<()> {
    let mut encoder = try!(Encoder::new(destination, level));
    let mut input = data;
    try!(io::copy(&mut input, &mut encoder));
    try!(encoder.finish());
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io;
    use super::{Decoder, Encoder};
    use super::{decode_all, encode_to_buffer};

    #[test]
    fn test_end_of_frame() {
        use std::io::{Read, Write};

        let mut enc = Encoder::new(Vec::new(), 1).unwrap();
        enc.write_all(b"foo").unwrap();
        let mut compressed = enc.finish().unwrap();

        // Add footer/whatever to underlying storage.
        compressed.push(0);

        // Drain zstd stream until end-of-frame.
        let mut dec = Decoder::new(&compressed[..]).unwrap().single_frame();
        let mut buf = Vec::new();
        dec.read_to_end(&mut buf).unwrap();
        assert_eq!(&buf, b"foo");
    }

    #[test]
    fn test_concatenated_frames() {

        let mut buffer = Vec::new();
        encode_to_buffer(b"foo", &mut buffer, 1).unwrap();
        encode_to_buffer(b"bar", &mut buffer, 2).unwrap();
        encode_to_buffer(b"baz", &mut buffer, 3).unwrap();

        assert_eq!(&decode_all(&buffer).unwrap(), b"foobarbaz");
    }

    #[test]
    fn test_flush() {
        use std::io::Write;

        let buf = Vec::new();
        let mut z = Encoder::new(buf, 19).unwrap();

        z.write_all(b"hello").unwrap();

        z.flush().unwrap(); // Might corrupt stream
        let buf = z.finish().unwrap();

        let s = decode_all(&buf[..]).unwrap();
        let s = ::std::str::from_utf8(&s).unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn test_invalid_frame() {
        use std::io::Read;

        // I really hope this data is invalid...
        let data = &[1u8, 2u8, 3u8, 4u8, 5u8];
        let mut dec = Decoder::new(&data[..]).unwrap();
        assert_eq!(dec.read_to_end(&mut Vec::new())
                       .err()
                       .map(|e| e.kind()),
                   Some(io::ErrorKind::Other));
    }

    #[test]
    fn test_incomplete_frame() {
        use std::io::{Read, Write};

        let mut enc = Encoder::new(Vec::new(), 1).unwrap();
        enc.write_all(b"This is a regular string").unwrap();
        let mut compressed = enc.finish().unwrap();

        let half_size = compressed.len() - 2;
        compressed.truncate(half_size);

        let mut dec = Decoder::new(&compressed[..]).unwrap();
        assert_eq!(dec.read_to_end(&mut Vec::new())
                       .err()
                       .map(|e| e.kind()),
                   Some(io::ErrorKind::UnexpectedEof));
    }

    #[test]
    fn test_legacy() {
        use std::fs;
        use std::io::Read;

        let mut target = Vec::new();

        // Read the content from that file
        fs::File::open("assets/example.txt")
            .unwrap()
            .read_to_end(&mut target)
            .unwrap();

        for version in &[5, 6, 7, 8] {
            let filename = format!("assets/example.txt.v{}.zst", version);
            let file = fs::File::open(filename).unwrap();
            let mut decoder = Decoder::new(file).unwrap();

            let mut buffer = Vec::new();
            decoder.read_to_end(&mut buffer).unwrap();

            assert!(target == buffer,
                    "Error decompressing legacy version {}",
                    version);
        }
    }
}
