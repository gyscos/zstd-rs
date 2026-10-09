// Shared by separate test binaries to isolate process-wide counters.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

/// Bytes currently held through the global allocator.
pub static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
/// Bytes ever requested from the global allocator (monotonic).
pub static TOTAL_BYTES: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Relaxed);
            TOTAL_BYTES.fetch_add(layout.size(), Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Relaxed);
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;
