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
/// Number of `dealloc` calls for pointers this allocator never handed out,
/// i.e. memory that was allocated by C's `malloc` but freed through Rust.
static FOREIGN_FREES: AtomicUsize = AtomicUsize::new(0);

/// Open-addressing table of live pointers, so `dealloc` can tell whether it
/// is being handed back memory that it actually allocated. Must not allocate
/// itself, hence the fixed-size static.
const SLOTS: usize = 1 << 16;
const EMPTY: usize = 0;
const TOMBSTONE: usize = 1;
static LIVE_PTRS: [AtomicUsize; SLOTS] =
    [const { AtomicUsize::new(EMPTY) }; SLOTS];

fn slot_for(ptr: usize) -> usize {
    (ptr >> 4).wrapping_mul(0x9E37_79B9_7F4A_7C15) % SLOTS
}

fn track(ptr: usize) {
    let mut i = slot_for(ptr);
    for _ in 0..SLOTS {
        let slot = &LIVE_PTRS[i];
        let cur = slot.load(Relaxed);
        if (cur == EMPTY || cur == TOMBSTONE)
            && slot.compare_exchange(cur, ptr, Relaxed, Relaxed).is_ok()
        {
            return;
        }
        i = (i + 1) % SLOTS;
    }
    panic!("pointer table full");
}

/// Returns whether `ptr` was live (and forgets it).
fn untrack(ptr: usize) -> bool {
    let mut i = slot_for(ptr);
    for _ in 0..SLOTS {
        let slot = &LIVE_PTRS[i];
        match slot.load(Relaxed) {
            EMPTY => return false,
            cur if cur == ptr => {
                slot.store(TOMBSTONE, Relaxed);
                return true;
            }
            _ => i = (i + 1) % SLOTS,
        }
    }
    false
}

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Relaxed);
            TOTAL_BYTES.fetch_add(layout.size(), Relaxed);
            track(ptr as usize);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if !untrack(ptr as usize) {
            // Not ours: record it and leak rather than corrupt the heap.
            FOREIGN_FREES.fetch_add(1, Relaxed);
            return;
        }
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

/// `ZSTD_copyDCtx` (and `ZSTD_copyCCtx`) copy the source context's allocator
/// callbacks into the destination, and `ZSTD_free*Ctx` frees the destination
/// struct through those callbacks. A clone must therefore be allocated with
/// the same allocator as its source, or it is freed through the wrong one -
/// which this allocator would report as a foreign free.
///
/// Only the decompression side can be exercised: `ZSTD_copyCCtx` requires
/// the source to have gone through `ZSTD_compressBegin`, which this crate
/// does not expose, so `CCtx::try_clone` always fails with `stage_wrong`.
fn clones_are_freed_through_the_source_allocator() {
    let live_before = LIVE_BYTES.load(Relaxed);
    let foreign_before = FOREIGN_FREES.load(Relaxed);
    {
        let dctx = DCtx::create();
        let clone = dctx.try_clone().expect("failed to clone DCtx");
        drop(clone);
        drop(dctx);
    }
    assert_eq!(
        FOREIGN_FREES.load(Relaxed),
        foreign_before,
        "a DCtx clone was freed through an allocator it was not allocated with"
    );
    assert_eq!(
        LIVE_BYTES.load(Relaxed),
        live_before,
        "DCtx clone leaked global-allocator memory"
    );
}

/// The allocation counters are process-wide, so everything runs from a
/// single test to keep other test threads from disturbing them.
#[test]
fn global_allocator_is_used_by_default() {
    default_contexts_use_global_allocator();
    one_shot_functions_use_global_allocator();
    dictionaries_use_global_allocator();
    clones_are_freed_through_the_source_allocator();
}
