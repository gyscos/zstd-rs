//! End-to-end: streaming Encoder/Decoder owning contexts that allocate through
//! Rust's global allocator (`CCtx::try_create_with_global_allocator` +
//! `with_owned_context`).
#![cfg(feature = "experimental")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{BufReader, Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use zstd::zstd_safe::{CCtx, DCtx};

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE_BYTES.fetch_add(layout.size(), Relaxed);
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
fn streaming_roundtrip_with_owned_global_allocator_contexts() {
    let input =
        b"the quick brown fox jumps over the lazy dog; ".repeat(10_000);

    let cctx = CCtx::try_create_with_global_allocator()
        .expect("failed to create CCtx");
    let live_with_cctx = LIVE_BYTES.load(Relaxed);

    let compressed = {
        let mut encoder = zstd::Encoder::with_owned_context(Vec::new(), cctx);
        encoder.write_all(&input).expect("compression failed");
        // The streaming workspace is allocated lazily on first write; by now
        // it must be visible to the global allocator (~1 MiB at the default
        // level).
        assert!(
            LIVE_BYTES.load(Relaxed) > live_with_cctx + 512 * 1024,
            "the encoder workspace is not visible to the global allocator"
        );
        encoder.finish().expect("failed to finish frame")
    };

    let dctx = DCtx::try_create_with_global_allocator()
        .expect("failed to create DCtx");
    let mut decoder = zstd::Decoder::with_owned_context(
        BufReader::new(compressed.as_slice()),
        dctx,
    );
    let mut roundtrip = Vec::with_capacity(input.len());
    decoder
        .read_to_end(&mut roundtrip)
        .expect("decompression failed");

    assert_eq!(input, roundtrip, "roundtrip mismatch");
}
