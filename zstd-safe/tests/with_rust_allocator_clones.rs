#![cfg(all(feature = "with-rust-allocator", feature = "std"))]

mod support {
    pub mod rust_allocator;
}

use std::sync::atomic::Ordering::Relaxed;
use support::rust_allocator::{FOREIGN_FREES, LIVE_BYTES};
use zstd_safe::{CCtx, DCtx};

#[test]
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
    assert_eq!(FOREIGN_FREES.load(Relaxed), foreign_before);
}
