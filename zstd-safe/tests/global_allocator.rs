//! Contexts created with `try_create_with_global_allocator` must route zstd's
//! internal allocations through Rust's global allocator, and free everything
//! through it on drop.
#![cfg(all(feature = "experimental", feature = "std"))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use zstd_safe::{CCtx, DCtx};

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static TOTAL_ALLOCS: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE_BYTES.fetch_add(layout.size(), Relaxed);
        TOTAL_ALLOCS.fetch_add(1, Relaxed);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Relaxed);
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[test]
fn context_allocations_go_through_global_allocator() {
    let input = b"hello zstd hello zstd hello zstd".repeat(256);
    // Pre-allocate the buffers so only the contexts allocate between the
    // measurement points below.
    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(input.len()));
    let mut decompressed = Vec::with_capacity(input.len());

    // --- compression context ---
    let live_before = LIVE_BYTES.load(Relaxed);
    let allocs_before = TOTAL_ALLOCS.load(Relaxed);
    {
        let mut cctx = CCtx::try_create_with_global_allocator()
            .expect("failed to create CCtx");
        cctx.compress(&mut compressed, &input, 3)
            .expect("compression failed");

        let held = LIVE_BYTES.load(Relaxed) - live_before;
        assert!(
            TOTAL_ALLOCS.load(Relaxed) > allocs_before,
            "the CCtx never hit the global allocator"
        );
        assert!(
            held > 1024,
            "expected the CCtx workspace to be held via the global \
             allocator, only {held} bytes live"
        );
    }
    // Dropping the context must return every byte through the same allocator.
    assert_eq!(
        LIVE_BYTES.load(Relaxed),
        live_before,
        "CCtx leaked global-allocator memory"
    );

    // --- decompression context ---
    let live_before = LIVE_BYTES.load(Relaxed);
    {
        let mut dctx = DCtx::try_create_with_global_allocator()
            .expect("failed to create DCtx");
        dctx.decompress(&mut decompressed, &compressed)
            .expect("decompression failed");
        assert!(
            LIVE_BYTES.load(Relaxed) > live_before,
            "the DCtx never hit the global allocator"
        );
    }
    assert_eq!(
        LIVE_BYTES.load(Relaxed),
        live_before,
        "DCtx leaked global-allocator memory"
    );

    assert_eq!(input, decompressed, "roundtrip mismatch");
}
