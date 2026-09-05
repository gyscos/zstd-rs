//! A `ZSTD_customMem` backed by Rust's global allocator.
//!
//! By default, zstd allocates its internal state (e.g. per-context compression
//! windows and match-finder tables, easily megabytes per context) with the C
//! runtime's `malloc`/`free`, invisible to Rust. Creating contexts with this
//! `ZSTD_customMem` routes those allocations through the Rust global allocator
//! instead, so custom `#[global_allocator]` implementations (tracking,
//! counting, jemalloc, ...) see them too.
//!
//! `ZSTD_customMem` is specified as a drop-in replacement for `malloc`/`free`:
//! zstd frees every allocation through the same `customFree` it allocated it
//! with, passes NULL to `customFree` freely, and relies on its own internal
//! re-alignment (`ZSTD_cwksp`) for anything stricter than `malloc` alignment.
//!
//! Rust's `dealloc` requires the original `Layout`, which `customFree` is not
//! given, so the allocation size is stored in a header just below the pointer
//! handed to zstd. This mirrors the `malloc` shim zstd-sys already uses on
//! wasm targets (`zstd-sys/src/wasm_shim.rs`), with a 16-byte header so the
//! returned pointer keeps the fundamental (`max_align_t`) alignment `malloc`
//! guarantees on 64-bit platforms.

extern crate alloc;

use alloc::alloc::{alloc, dealloc, Layout};
use core::ffi::c_void;
use core::ptr;

/// Size (and alignment) of the header stored below each allocation, holding
/// the full allocation size so the `Layout` can be rebuilt on free.
const HEADER: usize = 16;

unsafe extern "C" fn rust_alloc(
    _opaque: *mut c_void,
    size: usize,
) -> *mut c_void {
    let Some(total) = size.checked_add(HEADER) else {
        return ptr::null_mut();
    };
    // Safety: `total` is non-zero, does not overflow, and HEADER is a power
    // of two, so the layout is valid.
    let layout = Layout::from_size_align_unchecked(total, HEADER);
    let base = alloc(layout);
    if base.is_null() {
        // zstd surfaces NULL as a memory_allocation error; never unwind
        // through the FFI boundary.
        return ptr::null_mut();
    }
    base.cast::<usize>().write(total);
    base.add(HEADER).cast()
}

unsafe extern "C" fn rust_free(_opaque: *mut c_void, address: *mut c_void) {
    // ZSTD_customFree may be called with NULL.
    if address.is_null() {
        return;
    }
    // Safety: zstd only frees pointers returned by `rust_alloc`, which wrote
    // the allocation size right below the returned pointer.
    let base = address.cast::<u8>().sub(HEADER);
    let total = base.cast::<usize>().read();
    dealloc(base, Layout::from_size_align_unchecked(total, HEADER));
}

/// Allocator table passed to `ZSTD_create*_advanced()`.
pub(crate) const RUST_GLOBAL_ALLOCATOR: zstd_sys::ZSTD_customMem =
    zstd_sys::ZSTD_customMem {
        customAlloc: Some(rust_alloc),
        customFree: Some(rust_free),
        opaque: ptr::null_mut(),
    };
