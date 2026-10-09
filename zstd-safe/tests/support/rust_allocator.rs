// Shared by separate test binaries to isolate process-wide counters.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

/// Bytes currently held through the global allocator.
pub static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
/// Bytes ever requested from the global allocator (monotonic).
pub static TOTAL_BYTES: AtomicUsize = AtomicUsize::new(0);
/// Number of `dealloc` calls for pointers this allocator never handed out,
/// i.e. memory that was allocated by C's `malloc` but freed through Rust.
pub static FOREIGN_FREES: AtomicUsize = AtomicUsize::new(0);

/// Open-addressing table of live pointers, so `dealloc` can tell whether it
/// is being handed back memory that it actually allocated. Must not allocate
/// itself, hence the fixed-size static.
const SLOTS: usize = 1 << 16;
const EMPTY: usize = 0;
const TOMBSTONE: usize = 1;
// Rust 1.64 can repeat a named const containing an atomic, but cannot use
// the newer inline-const array syntax. Each slot gets a distinct atomic.
#[allow(clippy::declare_interior_mutable_const)]
const EMPTY_SLOT: AtomicUsize = AtomicUsize::new(EMPTY);
static LIVE_PTRS: [AtomicUsize; SLOTS] = [EMPTY_SLOT; SLOTS];

fn slot_for(ptr: usize) -> usize {
    (ptr >> 4).wrapping_mul(0x9E37_79B9) % SLOTS
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
    std::process::abort();
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
