use std::convert::TryInto;

#[no_mangle]
pub extern "C" fn zstd_version() -> u32 {
    unsafe { zstd_sys::ZSTD_versionNumber() }
}

macro_rules! zstd_check {
    ( $ret:expr ) => {{
        let ret = $ret;
        let error_code = unsafe { zstd_sys::ZSTD_isError(ret) };
        assert_eq!(error_code, 0);
    }};
}

#[no_mangle]
pub extern "C" fn test_compress() -> bool {
    let fbuf = include_bytes!("../Cargo.toml");

    let cbufsize = unsafe { zstd_sys::ZSTD_compressBound(fbuf.len()) };
    let mut cbuf = vec![0; cbufsize];

    let csize = unsafe {
        zstd_sys::ZSTD_compress(
            cbuf.as_mut_ptr().cast(),
            cbuf.len(),
            fbuf.as_ptr().cast(),
            fbuf.len(),
            1,
        )
    };
    zstd_check!(csize);
    let cbuf = &cbuf[..csize];

    let rsize = unsafe {
        zstd_sys::ZSTD_getFrameContentSize(cbuf.as_ptr().cast(), cbuf.len())
    };
    let rsize = rsize.try_into().unwrap();
    let mut rbuf = vec![0; rsize];

    let dsize = unsafe {
        zstd_sys::ZSTD_decompress(
            rbuf.as_mut_ptr().cast(),
            rbuf.len(),
            cbuf.as_ptr().cast(),
            cbuf.len(),
        )
    };
    zstd_check!(dsize);
    assert_eq!(dsize, rsize);

    &fbuf[..] == &rbuf[..]
}

fn main() {}
