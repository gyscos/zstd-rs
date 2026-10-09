#![cfg(feature = "with-rust-allocator")]

mod support {
    pub mod rust_allocator;
}

use std::io::{Read, Write};
use std::sync::atomic::Ordering::Relaxed;
use support::rust_allocator::LIVE_BYTES;

fn input() -> Vec<u8> {
    b"the quick brown fox jumps over the lazy dog; ".repeat(10_000)
}

const WORKSPACE_FLOOR: usize = 512 * 1024;

#[test]
fn stream_api_uses_global_allocator() {
    let input = input();

    let before = LIVE_BYTES.load(Relaxed);
    let compressed = {
        let mut encoder = zstd::Encoder::new(Vec::new(), 3).expect("encoder");
        encoder.write_all(&input).expect("compression failed");
        assert!(
            LIVE_BYTES.load(Relaxed) - before > WORKSPACE_FLOOR,
            "stream Encoder workspace is not visible to the global allocator"
        );
        encoder.finish().expect("failed to finish frame")
    };
    // Only the output Vec should still be alive.
    assert_eq!(
        LIVE_BYTES.load(Relaxed) - before,
        compressed.capacity(),
        "stream Encoder leaked"
    );

    let before = LIVE_BYTES.load(Relaxed);
    let mut roundtrip = Vec::with_capacity(input.len());
    {
        let mut decoder =
            zstd::Decoder::new(compressed.as_slice()).expect("decoder");
        decoder
            .read_to_end(&mut roundtrip)
            .expect("decompression failed");
        assert!(
            LIVE_BYTES.load(Relaxed) - before > roundtrip.capacity(),
            "stream Decoder is not visible to the global allocator"
        );
    }
    assert_eq!(
        LIVE_BYTES.load(Relaxed) - before,
        roundtrip.capacity(),
        "stream Decoder leaked"
    );
    assert_eq!(input, roundtrip);
}
