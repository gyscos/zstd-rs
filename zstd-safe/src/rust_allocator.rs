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
    let total = match size.checked_add(HEADER) {
        Some(total) => total,
        None => return ptr::null_mut(),
    };
    // Layout also checks that the aligned size fits in isize::MAX.
    let layout = match Layout::from_size_align(total, HEADER) {
        Ok(layout) => layout,
        Err(_) => return ptr::null_mut(),
    };
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

/// Equivalent of `ZSTD_createCDict()` / `ZSTD_createCDict_byReference()`
/// (depending on `load_method`), but allocating through the Rust global
/// allocator.
///
/// Mirrors zstd's own implementation: compression parameters are derived from
/// the level for an unknown source size and the dictionary size, and the
/// dictionary content type is auto-detected.
///
/// Note: unlike `ZSTD_createCDict()`, the `_advanced` constructor cannot
/// record the compression level inside the `CDict`. zstd uses that level to
/// re-derive parameters when compressing a large input of known size against
/// a small dictionary; dictionaries created here always use the parameters
/// they were built with instead, which is exactly what `ZSTD_CCtx_refCDict()`
/// does for streaming and for inputs of unknown size.
///
/// # Safety
///
/// With `ZSTD_dlm_byRef`, the caller must keep `dict_buffer` alive and
/// unmodified until the dictionary is freed. A non-null result must not be
/// freed while referenced by a context; free it at most once with
/// `ZSTD_freeCDict`.
pub(crate) unsafe fn create_cdict(
    dict_buffer: &[u8],
    compression_level: crate::CompressionLevel,
    load_method: zstd_sys::ZSTD_dictLoadMethod_e,
) -> *mut zstd_sys::ZSTD_CDict {
    let cparams = zstd_sys::ZSTD_getCParams(
        compression_level,
        crate::CONTENTSIZE_UNKNOWN as core::ffi::c_ulonglong,
        dict_buffer.len(),
    );
    zstd_sys::ZSTD_createCDict_advanced(
        crate::ptr_void(dict_buffer),
        dict_buffer.len(),
        load_method,
        zstd_sys::ZSTD_dictContentType_e::ZSTD_dct_auto,
        cparams,
        RUST_GLOBAL_ALLOCATOR,
    )
}

/// Equivalent of `ZSTD_createDDict()` / `ZSTD_createDDict_byReference()`
/// (depending on `load_method`), but allocating through the Rust global
/// allocator.
///
/// # Safety
///
/// With `ZSTD_dlm_byRef`, the caller must keep `dict_buffer` alive and
/// unmodified until the dictionary is freed. A non-null result must not be
/// freed while referenced by a context; free it at most once with
/// `ZSTD_freeDDict`.
pub(crate) unsafe fn create_ddict(
    dict_buffer: &[u8],
    load_method: zstd_sys::ZSTD_dictLoadMethod_e,
) -> *mut zstd_sys::ZSTD_DDict {
    zstd_sys::ZSTD_createDDict_advanced(
        crate::ptr_void(dict_buffer),
        dict_buffer.len(),
        load_method,
        zstd_sys::ZSTD_dictContentType_e::ZSTD_dct_auto,
        RUST_GLOBAL_ALLOCATOR,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_allocations_return_null() {
        unsafe {
            assert!(rust_alloc(ptr::null_mut(), usize::MAX).is_null());
            assert!(rust_alloc(ptr::null_mut(), isize::MAX as usize).is_null());
            assert!(rust_alloc(ptr::null_mut(), isize::MAX as usize - HEADER)
                .is_null());
        }
    }

    #[test]
    fn zero_sized_allocation_is_aligned_and_can_be_freed() {
        unsafe {
            let address = rust_alloc(ptr::null_mut(), 0);
            assert!(!address.is_null());
            assert_eq!(address as usize % HEADER, 0);
            rust_free(ptr::null_mut(), address);
            rust_free(ptr::null_mut(), ptr::null_mut());
        }
    }
}
