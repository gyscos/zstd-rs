#![cfg(all(feature = "with-rust-allocator", feature = "std"))]

mod support {
    pub mod rust_allocator;
}

use std::sync::atomic::Ordering::Relaxed;
use support::rust_allocator::LIVE_BYTES;
use zstd_safe::{CCtx, CDict, DCtx, DDict};

fn input() -> Vec<u8> {
    b"hello zstd hello zstd hello zstd".repeat(256)
}

#[test]
fn dictionaries_use_global_allocator() {
    let input = input();
    let dict: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(input.len()));
    let mut decompressed = Vec::with_capacity(input.len());

    let before = LIVE_BYTES.load(Relaxed);
    {
        let cdict = CDict::create(&dict, 3);
        let held = LIVE_BYTES.load(Relaxed) - before;
        assert!(
            held >= cdict.sizeof(),
            "CDict::create() did not allocate through the global allocator \
             ({held} < {})",
            cdict.sizeof()
        );
        let cdict_ref = CDict::create_by_reference(&dict, 3);
        assert!(
            LIVE_BYTES.load(Relaxed) - before > held,
            "CDict::create_by_reference() did not allocate through the \
             global allocator"
        );

        let mut cctx = CCtx::create();
        cctx.compress_using_cdict(&mut compressed, &input, &cdict)
            .expect("compression failed");
        drop(cdict_ref);
    }
    assert_eq!(LIVE_BYTES.load(Relaxed), before, "CDict leaked");

    let before = LIVE_BYTES.load(Relaxed);
    {
        let ddict = DDict::create(&dict);
        let held = LIVE_BYTES.load(Relaxed) - before;
        assert!(
            held >= ddict.sizeof(),
            "DDict::create() did not allocate through the global allocator \
             ({held} < {})",
            ddict.sizeof()
        );
        let ddict_ref = DDict::create_by_reference(&dict);
        assert!(
            LIVE_BYTES.load(Relaxed) - before > held,
            "DDict::create_by_reference() did not allocate through the \
             global allocator"
        );

        let mut dctx = DCtx::create();
        dctx.decompress_using_ddict(&mut decompressed, &compressed, &ddict)
            .expect("decompression failed");
        drop(ddict_ref);
    }
    assert_eq!(LIVE_BYTES.load(Relaxed), before, "DDict leaked");
    assert_eq!(input, decompressed);
}
