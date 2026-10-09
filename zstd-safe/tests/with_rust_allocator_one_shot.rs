#![cfg(all(feature = "with-rust-allocator", feature = "std"))]

mod support {
    pub mod rust_allocator;
}

use std::sync::atomic::Ordering::Relaxed;
use support::rust_allocator::{LIVE_BYTES, TOTAL_BYTES};

fn input() -> Vec<u8> {
    b"hello zstd hello zstd hello zstd".repeat(256)
}

#[test]
fn one_shot_functions_use_global_allocator() {
    let input = input();
    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(input.len()));
    let mut decompressed = Vec::with_capacity(input.len());

    // The one-shot calls allocate and free their context internally, so we
    // check the monotonic counter for the allocation and the live counter
    // for the matching free.
    let live = LIVE_BYTES.load(Relaxed);
    let total = TOTAL_BYTES.load(Relaxed);
    zstd_safe::compress(&mut compressed, &input, 3)
        .expect("compression failed");
    assert!(
        TOTAL_BYTES.load(Relaxed) - total > 1024,
        "compress() did not allocate through the global allocator"
    );
    assert_eq!(LIVE_BYTES.load(Relaxed), live, "compress() leaked");

    let live = LIVE_BYTES.load(Relaxed);
    let total = TOTAL_BYTES.load(Relaxed);
    zstd_safe::decompress(&mut decompressed, &compressed)
        .expect("decompression failed");
    assert!(
        TOTAL_BYTES.load(Relaxed) > total,
        "decompress() did not allocate through the global allocator"
    );
    assert_eq!(LIVE_BYTES.load(Relaxed), live, "decompress() leaked");
    assert_eq!(input, decompressed);
}
