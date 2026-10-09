//! Opaque handles retain their existing traits with either binding source.

fn assert_copy_clone<T: Copy + Clone>() {}

#[test]
fn opaque_handles_remain_copy_and_clone() {
    assert_copy_clone::<zstd_sys::ZSTD_CCtx_s>();
    assert_copy_clone::<zstd_sys::ZSTD_DCtx_s>();
    assert_copy_clone::<zstd_sys::ZSTD_CDict_s>();
    assert_copy_clone::<zstd_sys::ZSTD_DDict_s>();

    #[cfg(feature = "experimental")]
    {
        assert_copy_clone::<zstd_sys::ZSTD_CCtx_params_s>();
        assert_copy_clone::<zstd_sys::POOL_ctx_s>();
    }

    #[cfg(feature = "seekable")]
    {
        assert_copy_clone::<zstd_sys::ZSTD_seekable_CStream_s>();
        assert_copy_clone::<zstd_sys::ZSTD_seekable_s>();
        assert_copy_clone::<zstd_sys::ZSTD_seekTable_s>();
        assert_copy_clone::<zstd_sys::ZSTD_frameLog_s>();
    }
}
