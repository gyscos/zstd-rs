#![cfg(feature = "with-rust-allocator")]

mod support {
    pub mod rust_allocator;
}

use std::io::{Read, Write};
use std::sync::atomic::Ordering::Relaxed;
use support::rust_allocator::{LIVE_BYTES, TOTAL_BYTES};

fn input() -> Vec<u8> {
    b"the quick brown fox jumps over the lazy dog; ".repeat(10_000)
}

const WORKSPACE_FLOOR: usize = 512 * 1024;

#[test]
fn dictionaries_use_global_allocator() {
    let input = input();
    let dict: Vec<u8> = (0..=255u8).cycle().take(4096).collect();

    let before = LIVE_BYTES.load(Relaxed);
    let compressed = {
        let mut encoder = zstd::Encoder::with_dictionary(Vec::new(), 3, &dict)
            .expect("encoder");
        encoder.write_all(&input).expect("compression failed");
        assert!(
            LIVE_BYTES.load(Relaxed) - before > WORKSPACE_FLOOR,
            "Encoder::with_dictionary workspace is not visible to the \
             global allocator"
        );
        encoder.finish().expect("failed to finish frame")
    };
    assert_eq!(
        LIVE_BYTES.load(Relaxed) - before,
        compressed.capacity(),
        "Encoder::with_dictionary leaked"
    );

    let before = LIVE_BYTES.load(Relaxed);
    {
        let prepared = zstd::dict::EncoderDictionary::copy(&dict, 3);
        assert!(
            LIVE_BYTES.load(Relaxed) - before >= prepared.as_cdict().sizeof(),
            "EncoderDictionary::copy is not visible to the global allocator"
        );
        let prepared = zstd::dict::DecoderDictionary::copy(&dict);
        assert!(
            LIVE_BYTES.load(Relaxed) - before >= prepared.as_ddict().sizeof(),
            "DecoderDictionary::copy is not visible to the global allocator"
        );
    }
    assert_eq!(
        LIVE_BYTES.load(Relaxed),
        before,
        "prepared dictionaries leaked"
    );

    let before = LIVE_BYTES.load(Relaxed);
    let mut roundtrip = Vec::with_capacity(input.len());
    {
        let prepared = zstd::dict::DecoderDictionary::copy(&dict);
        let mut decoder = zstd::Decoder::with_prepared_dictionary(
            compressed.as_slice(),
            &prepared,
        )
        .expect("decoder");
        decoder
            .read_to_end(&mut roundtrip)
            .expect("decompression failed");
    }
    assert_eq!(
        LIVE_BYTES.load(Relaxed) - before,
        roundtrip.capacity(),
        "Decoder::with_prepared_dictionary leaked"
    );
    assert_eq!(input, roundtrip);

    // One-shot helpers.
    let total = TOTAL_BYTES.load(Relaxed);
    let live = LIVE_BYTES.load(Relaxed);
    let compressed =
        zstd::encode_all(input.as_slice(), 3).expect("encode_all");
    let decoded = zstd::decode_all(compressed.as_slice()).expect("decode_all");
    assert!(TOTAL_BYTES.load(Relaxed) - total > WORKSPACE_FLOOR);
    drop((compressed, decoded));
    assert_eq!(
        LIVE_BYTES.load(Relaxed),
        live,
        "encode_all/decode_all leaked"
    );
}
