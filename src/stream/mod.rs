pub mod encoder;
pub mod decoder;


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
