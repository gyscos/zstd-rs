#![cfg(feature = "with-rust-allocator")]

mod support {
    pub mod rust_allocator;
}

use std::sync::atomic::Ordering::Relaxed;
use support::rust_allocator::LIVE_BYTES;

fn input() -> Vec<u8> {
    b"the quick brown fox jumps over the lazy dog; ".repeat(10_000)
}

const WORKSPACE_FLOOR: usize = 512 * 1024;

#[test]
fn bulk_api_uses_global_allocator() {
    let input = input();
    let mut compressed = Vec::with_capacity(input.len());
    let mut decompressed = vec![0u8; input.len()];

    let before = LIVE_BYTES.load(Relaxed);
    {
        let mut compressor =
            zstd::bulk::Compressor::new(3).expect("compressor");
        compressor
            .compress_to_buffer(&input, &mut compressed)
            .expect("compression failed");
        assert!(
            LIVE_BYTES.load(Relaxed) - before > WORKSPACE_FLOOR,
            "bulk::Compressor workspace is not visible to the global allocator"
        );
    }
    assert_eq!(LIVE_BYTES.load(Relaxed), before, "bulk::Compressor leaked");

    let before = LIVE_BYTES.load(Relaxed);
    {
        let mut decompressor =
            zstd::bulk::Decompressor::new().expect("decompressor");
        let n = decompressor
            .decompress_to_buffer(&compressed, &mut decompressed[..])
            .expect("decompression failed");
        assert_eq!(n, input.len());
        assert!(
            LIVE_BYTES.load(Relaxed) > before,
            "bulk::Decompressor is not visible to the global allocator"
        );
    }
    assert_eq!(
        LIVE_BYTES.load(Relaxed),
        before,
        "bulk::Decompressor leaked"
    );
    assert_eq!(input, decompressed);
}
