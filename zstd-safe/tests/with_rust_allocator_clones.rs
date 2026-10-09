#![cfg(all(feature = "with-rust-allocator", feature = "std"))]

mod support {
    pub mod rust_allocator;
}

use std::sync::atomic::Ordering::Relaxed;
use support::rust_allocator::{LIVE_BYTES, TOTAL_BYTES};
use zstd_safe::{CCtx, DCtx};

#[test]
fn clones_are_freed_through_the_source_allocator() {
    let live_before = LIVE_BYTES.load(Relaxed);
    {
        let dctx = DCtx::create();
        let total_before = TOTAL_BYTES.load(Relaxed);
        let clone = dctx.try_clone().expect("failed to clone DCtx");
        assert!(
            TOTAL_BYTES.load(Relaxed) > total_before,
            "DCtx clone did not allocate through Rust's allocator"
        );
        drop(dctx);
        drop(clone);
    }
    assert_eq!(
        LIVE_BYTES.load(Relaxed),
        live_before,
        "DCtx clone leaked global-allocator memory"
    );

    // Copying an uninitialized CCtx fails. The freshly allocated destination
    // must still be freed through Rust's allocator on this error path.
    {
        let cctx = CCtx::create();
        let before = LIVE_BYTES.load(Relaxed);
        assert!(cctx.try_clone(None).is_err());
        assert_eq!(
            LIVE_BYTES.load(Relaxed),
            before,
            "failed CCtx clone leaked"
        );
    }
    assert_eq!(LIVE_BYTES.load(Relaxed), live_before);
}
