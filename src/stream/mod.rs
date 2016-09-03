//! Compress and decompress Zstd streams.
//!
//! This module provide a `Read`/`Write` interface to zstd streams of arbitrary length.
//!
//! They are compatible with the `zstd` command-line tool.

mod encoder;
mod decoder;

pub use self::encoder::{AutoFinishEncoder, Encoder};
pub use self::decoder::Decoder;

#[cfg(test)]
mod tests {
    use super::{Decoder, Encoder};

    #[test]
    fn test_end_of_frame() {
        use std::io::{Read, Write};

        let mut enc = Encoder::new(Vec::new(), 1).unwrap();
        enc.write_all(b"foo").unwrap();
        let mut compressed = enc.finish().unwrap();

        // Add footer/whatever to underlying storage.
        compressed.push(0);

        // Drain zstd stream until end-of-frame.
        let mut dec = Decoder::new(&compressed[..]).unwrap();
        let mut buf = Vec::new();
        dec.read_to_end(&mut buf).unwrap();
        assert_eq!(&buf, b"foo");
    }

    #[test]
    fn test_flush() {
        use std::io::Write;

        let buf = Vec::new();
        let mut z = Encoder::new(buf, 19).unwrap();

        z.write_all(b"hello").unwrap();

        z.flush().unwrap(); // Might corrupt stream
        let buf = z.finish().unwrap();

        let s = ::decode_all(&buf[..]).unwrap();
        let s = ::std::str::from_utf8(&s).unwrap();
        assert_eq!(s, "hello");
    }

    #[test]
    fn test_invalid_frame() {
        use std::io::Read;

        // I really hope this data is invalid...
        let data = &[1u8, 2u8, 3u8, 4u8, 5u8];
        let mut dec = Decoder::new(&data[..]).unwrap();
        assert!(dec.read_to_end(&mut Vec::new()).is_err());
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
        assert!(dec.read_to_end(&mut Vec::new()).is_err());
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

            assert!(target == buffer, "Error decompressing legacy version {}", version);
        }
    }
}
