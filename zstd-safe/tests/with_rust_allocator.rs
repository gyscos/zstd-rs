#![cfg(all(feature = "with-rust-allocator", feature = "std"))]

mod support {
    pub mod rust_allocator;
}

use std::sync::atomic::Ordering::Relaxed;
use support::rust_allocator::LIVE_BYTES;
use zstd_safe::{CCtx, DCtx};

fn input() -> Vec<u8> {
    b"hello zstd hello zstd hello zstd".repeat(256)
}

#[test]
fn default_contexts_use_global_allocator() {
    let input = input();
    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(input.len()));
    let mut decompressed = Vec::with_capacity(input.len());

    let before = LIVE_BYTES.load(Relaxed);
    {
        let mut cctx = CCtx::create();
        cctx.compress(&mut compressed, &input, 3)
            .expect("compression failed");
        assert!(
            LIVE_BYTES.load(Relaxed) - before > 1024,
            "CCtx::create() did not allocate through the global allocator"
        );
    }
    assert_eq!(LIVE_BYTES.load(Relaxed), before, "CCtx leaked");

    let before = LIVE_BYTES.load(Relaxed);
    {
        let mut dctx = DCtx::default();
        dctx.decompress(&mut decompressed, &compressed)
            .expect("decompression failed");
        assert!(
            LIVE_BYTES.load(Relaxed) > before,
            "DCtx::default() did not allocate through the global allocator"
        );
    }
    assert_eq!(LIVE_BYTES.load(Relaxed), before, "DCtx leaked");
    assert_eq!(input, decompressed);
}
