use crate::stream::read::{Decoder, Encoder};
use std::io::Read;

#[test]
fn test_error_handling() {
    let invalid_input = b"Abcdefghabcdefgh";

    let mut decoder = Decoder::new(&invalid_input[..]).unwrap();
    let output = decoder.read_to_end(&mut Vec::new());

    assert!(output.is_err());
}

#[test]
fn test_cycle() {
    let input = b"Abcdefghabcdefgh";

    let mut encoder = Encoder::new(&input[..], 1).unwrap();
    let mut buffer = Vec::new();
    encoder.read_to_end(&mut buffer).unwrap();

    let mut decoder = Decoder::new(&buffer[..]).unwrap();
    let mut buffer = Vec::new();
    decoder.read_to_end(&mut buffer).unwrap();

    assert_eq!(input, &buffer[..]);
}

#[test]
fn test_read_to_end_over_several_blocks() {
    // The `NoOp` test in `zio::reader` covers the buffer bookkeeping on its
    // own. This puts real block boundaries under it, which is what the
    // chunked initialization actually has to interleave with.
    let block = zstd_safe::BLOCKSIZE_MAX as usize;
    let input: Vec<u8> =
        (0..block * 2 + 1234).map(|i| (i % 251) as u8).collect();
    let compressed = crate::bulk::compress(&input, 1).unwrap();

    // Vary what the destination already holds, and how much room it has: the
    // implementation treats "no spare capacity" and "some spare capacity"
    // differently, and has to append rather than overwrite either way.
    for spare in [0, 1, block, input.len() + 1] {
        let mut output = Vec::with_capacity(6 + spare);
        output.extend_from_slice(b"prefix");

        let mut decoder = Decoder::new(&compressed[..]).unwrap();
        let read = decoder.read_to_end(&mut output).unwrap();

        assert_eq!(read, input.len(), "spare {}", spare);
        assert_eq!(&output[..6], b"prefix", "spare {}", spare);
        assert_eq!(&output[6..], &input[..], "spare {}", spare);

        // Reading again after the end appends nothing.
        assert_eq!(decoder.read_to_end(&mut output).unwrap(), 0);
        assert_eq!(output.len(), 6 + input.len());
    }
}

#[test]
fn test_read_to_end_leaves_no_padding_on_error() {
    // A frame that stops early: whatever was decoded stays, but none of the
    // space the implementation initialized may show up as data.
    let input = vec![7u8; 100_000];
    let compressed = crate::bulk::compress(&input, 1).unwrap();
    let truncated = &compressed[..compressed.len() / 2];

    // Spare capacity matters: with none, the implementation probes with a
    // stack buffer and never initializes anything. It is the resize path that
    // could leave zeros behind.
    let mut output = Vec::with_capacity(6 + input.len());
    output.extend_from_slice(b"prefix");
    let mut decoder = Decoder::new(truncated).unwrap();

    assert!(decoder.read_to_end(&mut output).is_err());
    assert_eq!(&output[..6], b"prefix");
    assert!(
        input.starts_with(&output[6..]),
        "padding leaked into the output"
    );
}
