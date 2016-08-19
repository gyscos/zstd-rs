pub mod encoder;
pub mod decoder;

#[cfg(test)]
mod tests {
    use super::{decoder, encoder};

    #[test]
    fn test_end_of_frame() {
        use std::io::{Read, Write};

        let mut enc = encoder::Encoder::new(Vec::new(), 1).unwrap();
        enc.write_all(b"foo").unwrap();
        let mut compressed = enc.finish().unwrap();

        // Add footer/whatever to underlying storage.
        compressed.push(0);

        // Drain zstd stream until end-of-frame.
        let mut dec = decoder::Decoder::new(&compressed[..]).unwrap();
        let mut buf = Vec::new();
        dec.read_to_end(&mut buf).unwrap();
        assert_eq!(&buf, b"foo");
    }

    #[test]
    fn test_flush() {
        use std::io::Write;

        let buf = Vec::new();
        let mut z = encoder::Encoder::new(buf, 19).unwrap();

        z.write_all(b"hello").unwrap();

        z.flush().unwrap(); // Might corrupt stream
        let buf = z.finish().unwrap();

        let s = ::decode_all(&buf[..]).unwrap();
        let s = ::std::str::from_utf8(&s).unwrap();
        assert_eq!(s, "hello");
    }
}
