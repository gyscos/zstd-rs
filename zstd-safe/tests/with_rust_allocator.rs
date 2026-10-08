//! With the `with-rust-allocator` feature, the *default* constructors
//! (`CCtx::create`, `DCtx::create`, `CDict::create`, `DDict::create`, the
//! one-shot `compress`/`decompress`) must route zstd's allocations through
//! Rust's global allocator, with no opt-in from the caller.
#![cfg(all(feature = "with-rust-allocator", feature = "std"))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use zstd_safe::{CCtx, CDict, DCtx, DDict};

/// Bytes currently held through the global allocator.
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
/// Bytes ever requested from the global allocator (monotonic).
static TOTAL_BYTES: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE_BYTES.fetch_add(layout.size(), Relaxed);
        TOTAL_BYTES.fetch_add(layout.size(), Relaxed);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Relaxed);
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn input() -> Vec<u8> {
    b"hello zstd hello zstd hello zstd".repeat(256)
}

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

/// The allocation counters are process-wide, so everything runs from a
/// single test to keep other test threads from disturbing them.
#[test]
fn global_allocator_is_used_by_default() {
    default_contexts_use_global_allocator();
    one_shot_functions_use_global_allocator();
    dictionaries_use_global_allocator();
}
