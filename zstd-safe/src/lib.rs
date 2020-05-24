#![no_std]
//! Minimal safe wrapper around zstd-sys.
//!
//! This crates provides a minimal translation of the [zstd-sys] methods.
//! For a more comfortable high-level library, see the [zstd] crate.
//!
//! [zstd-sys]: http://crates.io/crates/zstd-sys
//! [zstd]: https://crates.io/crates/zstd
//!
//! Most of the functions here map 1-for-1 to a function from
//! [the C zstd library][zstd-c] mentionned in their descriptions.
//! Check the [source documentation][doc] for more information on their
//! behaviour.
//!
//! [doc]: http://facebook.github.io/zstd/zstd_manual.html
//! [zstd-c]: https://facebook.github.io/zstd/
//!
//! Features denoted as experimental in the C library are hidden behind an
//! `experimental` feature.

extern crate libc;
extern crate zstd_sys;

#[cfg(feature = "std")]
extern crate std;

#[cfg(test)]
mod tests;

/// How to compress data.
pub use zstd_sys::ZSTD_strategy as Strategy;

/// Reset directive.
pub use zstd_sys::ZSTD_ResetDirective as ResetDirective;

#[cfg(feature = "std")]
use std::os::raw::{c_char, c_int, c_ulonglong, c_void};

#[cfg(not(feature = "std"))]
use libc::{c_char, c_int, c_ulonglong, c_void};

use core::marker::PhantomData;
use core::ops::Deref;
use core::ops::DerefMut;
use core::str;

// Re-define constants from zstd_sys
pub const VERSION_MAJOR: u32 = zstd_sys::ZSTD_VERSION_MAJOR;
pub const VERSION_MINOR: u32 = zstd_sys::ZSTD_VERSION_MINOR;
pub const VERSION_RELEASE: u32 = zstd_sys::ZSTD_VERSION_RELEASE;
pub const VERSION_NUMBER: u32 = zstd_sys::ZSTD_VERSION_NUMBER;

/// Default compression level.
pub const CLEVEL_DEFAULT: CompressionLevel =
    zstd_sys::ZSTD_CLEVEL_DEFAULT as CompressionLevel;
pub const CONTENTSIZE_UNKNOWN: u64 = zstd_sys::ZSTD_CONTENTSIZE_UNKNOWN as u64;
pub const CONTENTSIZE_ERROR: u64 = zstd_sys::ZSTD_CONTENTSIZE_ERROR as u64;
pub const MAGICNUMBER: u32 = zstd_sys::ZSTD_MAGICNUMBER;
pub const MAGIC_DICTIONARY: u32 = zstd_sys::ZSTD_MAGIC_DICTIONARY;
pub const MAGIC_SKIPPABLE_START: u32 = zstd_sys::ZSTD_MAGIC_SKIPPABLE_START;
pub const BLOCKSIZELOG_MAX: u32 = zstd_sys::ZSTD_BLOCKSIZELOG_MAX;
pub const BLOCKSIZE_MAX: u32 = zstd_sys::ZSTD_BLOCKSIZE_MAX;
#[cfg(feature = "experimental")]
pub const WINDOWLOG_MAX_32: u32 = zstd_sys::ZSTD_WINDOWLOG_MAX_32;
#[cfg(feature = "experimental")]
pub const WINDOWLOG_MAX_64: u32 = zstd_sys::ZSTD_WINDOWLOG_MAX_64;
#[cfg(feature = "experimental")]
pub const WINDOWLOG_MIN: u32 = zstd_sys::ZSTD_WINDOWLOG_MIN;
#[cfg(feature = "experimental")]
pub const HASHLOG_MIN: u32 = zstd_sys::ZSTD_HASHLOG_MIN;
#[cfg(feature = "experimental")]
pub const CHAINLOG_MAX_32: u32 = zstd_sys::ZSTD_CHAINLOG_MAX_32;
#[cfg(feature = "experimental")]
pub const CHAINLOG_MAX_64: u32 = zstd_sys::ZSTD_CHAINLOG_MAX_64;
#[cfg(feature = "experimental")]
pub const CHAINLOG_MIN: u32 = zstd_sys::ZSTD_CHAINLOG_MIN;
#[cfg(feature = "experimental")]
pub const HASHLOG3_MAX: u32 = zstd_sys::ZSTD_HASHLOG3_MAX;
#[cfg(feature = "experimental")]
pub const SEARCHLOG_MIN: u32 = zstd_sys::ZSTD_SEARCHLOG_MIN;
#[cfg(feature = "experimental")]
pub const TARGETLENGTH_MAX: u32 = zstd_sys::ZSTD_TARGETLENGTH_MAX;
#[cfg(feature = "experimental")]
pub const TARGETLENGTH_MIN: u32 = zstd_sys::ZSTD_TARGETLENGTH_MIN;
#[cfg(feature = "experimental")]
pub const LDM_MINMATCH_MAX: u32 = zstd_sys::ZSTD_LDM_MINMATCH_MAX;
#[cfg(feature = "experimental")]
pub const LDM_MINMATCH_MIN: u32 = zstd_sys::ZSTD_LDM_MINMATCH_MIN;
#[cfg(feature = "experimental")]
pub const LDM_BUCKETSIZELOG_MAX: u32 = zstd_sys::ZSTD_LDM_BUCKETSIZELOG_MAX;

/// Represents the compression level used by zstd.
pub type CompressionLevel = i32;

/// Represents a possible error from the zstd library.
pub type ErrorCode = usize;

/// Wrapper result around most zstd functions.
///
/// Either a success code (usually number of bytes written), or an error code.
pub type SafeResult = Result<usize, ErrorCode>;

/// Returns true if code represents error.
fn is_error(code: usize) -> bool {
    unsafe { zstd_sys::ZSTD_isError(code) != 0 }
}

/// Parse the result code
///
/// Returns the number of bytes written if the code represents success,
/// or the error message code otherwise.
fn parse_code(code: usize) -> SafeResult {
    if !is_error(code) {
        Ok(code)
    } else {
        Err(code)
    }
}

fn ptr_void(src: &[u8]) -> *const c_void {
    src.as_ptr() as *const c_void
}

fn ptr_mut_void(dst: &mut [u8]) -> *mut c_void {
    dst.as_mut_ptr() as *mut c_void
}

pub fn version_number() -> u32 {
    unsafe { zstd_sys::ZSTD_versionNumber() as u32 }
}

pub fn version_string() -> &'static str {
    unsafe { c_char_to_str(zstd_sys::ZSTD_versionString()) }
}

/// Returns the minimum (fastest) compression level supported.
pub fn min_c_level() -> CompressionLevel {
    unsafe { zstd_sys::ZSTD_minCLevel() as CompressionLevel }
}

/// Returns the maximum (slowest) compression level supported.
pub fn max_c_level() -> CompressionLevel {
    unsafe { zstd_sys::ZSTD_maxCLevel() as CompressionLevel }
}

/// Wraps the `ZSTD_compress` function.
pub fn compress(
    dst: &mut [u8],
    src: &[u8],
    compression_level: CompressionLevel,
) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_compress(
            ptr_mut_void(dst),
            dst.len(),
            ptr_void(src),
            src.len(),
            compression_level,
        )
    };
    parse_code(code)
}

/// Wraps the `ZSTD_decompress` function.
pub fn decompress(dst: &mut [u8], src: &[u8]) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_decompress(
            ptr_mut_void(dst),
            dst.len(),
            ptr_void(src),
            src.len(),
        )
    };
    parse_code(code)
}

/// Wraps the `ZSTD_getDecompressedSize` function.
#[deprecated(note = "Use ZSTD_getFrameContentSize instead")]
pub fn get_decompressed_size(src: &[u8]) -> u64 {
    unsafe {
        zstd_sys::ZSTD_getDecompressedSize(ptr_void(src), src.len()) as u64
    }
}

/// maximum compressed size in worst case single-pass scenario
pub fn compress_bound(src_size: usize) -> usize {
    unsafe { zstd_sys::ZSTD_compressBound(src_size) }
}

pub struct CCtx<'a>(*mut zstd_sys::ZSTD_CCtx, PhantomData<&'a ()>);

impl<'a> Default for CCtx<'a> {
    fn default() -> Self {
        create_cctx()
    }
}

pub fn create_cctx<'a>() -> CCtx<'a> {
    CCtx(unsafe { zstd_sys::ZSTD_createCCtx() }, PhantomData)
}

impl<'a> Drop for CCtx<'a> {
    fn drop(&mut self) {
        unsafe {
            zstd_sys::ZSTD_freeCCtx(self.0);
        }
    }
}

unsafe impl<'a> Send for CCtx<'a> {}
// CCtx can't be shared across threads, so it does not implement Sync.

unsafe fn c_char_to_str(text: *const c_char) -> &'static str {
    #[cfg(not(feature = "std"))]
    {
        // To be safe, we need to compute right now its length
        let len = libc::strlen(text);

        // Cast it to a slice
        let slice = core::slice::from_raw_parts(text as *mut u8, len);
        // And hope it's still text.
        str::from_utf8(slice).expect("bad error message from zstd")
    }

    #[cfg(feature = "std")]
    {
        std::ffi::CStr::from_ptr(text)
            .to_str()
            .expect("bad error message from zstd")
    }
}

pub fn get_error_name(code: usize) -> &'static str {
    unsafe {
        let name = zstd_sys::ZSTD_getErrorName(code);
        c_char_to_str(name)
    }
}

/// Wraps the `ZSTD_compressCCtx()` function
pub fn compress_cctx(
    ctx: &mut CCtx,
    dst: &mut [u8],
    src: &[u8],
    compression_level: CompressionLevel,
) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_compressCCtx(
            ctx.0,
            ptr_mut_void(dst),
            dst.len(),
            ptr_void(src),
            src.len(),
            compression_level,
        )
    };
    parse_code(code)
}

/// Wraps the `ZSTD_compress2()` function.
pub fn compress2(ctx: &mut CCtx, dst: &mut [u8], src: &[u8]) -> SafeResult {
    parse_code(unsafe {
        zstd_sys::ZSTD_compress2(
            ctx.0,
            ptr_mut_void(dst),
            dst.len(),
            ptr_void(src),
            src.len(),
        )
    })
}

/// A Decompression Context.
///
/// The lifetime references the potential dictionary used for this context.
///
/// If no dictionary was used, it will most likely be `'static`.
///
/// Same as `DStream`.
pub struct DCtx<'a>(*mut zstd_sys::ZSTD_DCtx, PhantomData<&'a ()>);

impl Default for DCtx<'_> {
    fn default() -> Self {
        create_dctx()
    }
}

/// Prepares a new decompression context without dictionary.
pub fn create_dctx() -> DCtx<'static> {
    DCtx(unsafe { zstd_sys::ZSTD_createDCtx() }, PhantomData)
}

impl Drop for DCtx<'_> {
    fn drop(&mut self) {
        unsafe {
            zstd_sys::ZSTD_freeDCtx(self.0);
        }
    }
}

unsafe impl Send for DCtx<'_> {}
// DCtx can't be shared across threads, so it does not implement Sync.

/// Wraps the `ZSTD_decompressDCtx()` function.
pub fn decompress_dctx(
    ctx: &mut DCtx,
    dst: &mut [u8],
    src: &[u8],
) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_decompressDCtx(
            ctx.0,
            ptr_mut_void(dst),
            dst.len(),
            ptr_void(src),
            src.len(),
        )
    };
    parse_code(code)
}

/// Wraps the `ZSTD_compress_usingDict()` function.
pub fn compress_using_dict(
    ctx: &mut CCtx,
    dst: &mut [u8],
    src: &[u8],
    dict: &[u8],
    compression_level: CompressionLevel,
) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_compress_usingDict(
            ctx.0,
            ptr_mut_void(dst),
            dst.len(),
            ptr_void(src),
            src.len(),
            ptr_void(dict),
            dict.len(),
            compression_level,
        )
    };
    parse_code(code)
}

/// Wraps the `ZSTD_decompress_usingDict()` function.
pub fn decompress_using_dict(
    dctx: &mut DCtx,
    dst: &mut [u8],
    src: &[u8],
    dict: &[u8],
) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_decompress_usingDict(
            dctx.0,
            ptr_mut_void(dst),
            dst.len(),
            ptr_void(src),
            src.len(),
            ptr_void(dict),
            dict.len(),
        )
    };
    parse_code(code)
}

/// Compression dictionary.
pub struct CDict<'a>(*mut zstd_sys::ZSTD_CDict, PhantomData<&'a ()>);

/// Wraps the `ZSTD_createCDict()` function.
pub fn create_cdict(
    dict_buffer: &[u8],
    compression_level: CompressionLevel,
) -> CDict<'static> {
    CDict(
        unsafe {
            zstd_sys::ZSTD_createCDict(
                ptr_void(dict_buffer),
                dict_buffer.len(),
                compression_level,
            )
        },
        PhantomData,
    )
}

impl<'a> Drop for CDict<'a> {
    fn drop(&mut self) {
        unsafe {
            zstd_sys::ZSTD_freeCDict(self.0);
        }
    }
}

unsafe impl<'a> Send for CDict<'a> {}
unsafe impl<'a> Sync for CDict<'a> {}

/// Wraps the `ZSTD_compress_usingCDict()` function.
pub fn compress_using_cdict(
    cctx: &mut CCtx,
    dst: &mut [u8],
    src: &[u8],
    cdict: &CDict,
) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_compress_usingCDict(
            cctx.0,
            ptr_mut_void(dst),
            dst.len(),
            ptr_void(src),
            src.len(),
            cdict.0,
        )
    };
    parse_code(code)
}

pub struct DDict<'a>(*mut zstd_sys::ZSTD_DDict, PhantomData<&'a ()>);

/// Wraps the `ZSTD_createDDict()` function.
///
/// It copies the dictionary internally, so the resulting `DDict` is `'static`.
pub fn create_ddict(dict_buffer: &[u8]) -> DDict<'static> {
    DDict(
        unsafe {
            zstd_sys::ZSTD_createDDict(
                ptr_void(dict_buffer),
                dict_buffer.len(),
            )
        },
        PhantomData,
    )
}

impl<'a> Drop for DDict<'a> {
    fn drop(&mut self) {
        unsafe {
            zstd_sys::ZSTD_freeDDict(self.0);
        }
    }
}

unsafe impl<'a> Send for DDict<'a> {}
unsafe impl<'a> Sync for DDict<'a> {}

/// Wraps the `ZSTD_decompress_usingDDict()` function.
pub fn decompress_using_ddict(
    dctx: &mut DCtx,
    dst: &mut [u8],
    src: &[u8],
    ddict: &DDict,
) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_decompress_usingDDict(
            dctx.0,
            ptr_mut_void(dst),
            dst.len(),
            ptr_void(src),
            src.len(),
            ddict.0,
        )
    };
    parse_code(code)
}

/// Compression stream.
///
/// Same as `CCtx`.
pub type CStream<'a> = CCtx<'a>;

// CStream can't be shared across threads, so it does not implement Sync.

/// Allocates a new `CStream`.
pub fn create_cstream<'a>() -> CStream<'a> {
    CCtx(unsafe { zstd_sys::ZSTD_createCStream() }, PhantomData)
}

/// Prepares an existing `CStream` for compression at the given level.
pub fn init_cstream(
    zcs: &mut CStream,
    compression_level: CompressionLevel,
) -> usize {
    unsafe { zstd_sys::ZSTD_initCStream(zcs.0, compression_level) }
}

#[derive(Debug)]
/// Wrapper around an input buffer.
///
/// Bytes will be read starting at `src[pos]`.
///
/// `pos` will be updated after reading.
pub struct InBuffer<'a> {
    pub src: &'a [u8],
    pub pos: usize,
}

#[derive(Debug)]
/// Wrapper around an output buffer.
///
/// Bytes will be written starting at `dst[pos]`.
///
/// `pos` will be updated after writing.
pub struct OutBuffer<'a> {
    pub dst: &'a mut [u8],
    pub pos: usize,
}

/// Convenience method to get a mut pointer from a mut ref.
fn ptr_mut<B>(ptr_void: &mut B) -> *mut B {
    ptr_void as *mut B
}

/// Interface between a C-level ZSTD_outBuffer and a rust-level `OutBuffer`.
///
/// Will update the parent buffer from the C buffer on drop.
struct OutBufferWrapper<'a, 'b: 'a> {
    buf: zstd_sys::ZSTD_outBuffer,
    parent: &'a mut OutBuffer<'b>,
}

impl<'a, 'b: 'a> Deref for OutBufferWrapper<'a, 'b> {
    type Target = zstd_sys::ZSTD_outBuffer;

    fn deref(&self) -> &Self::Target {
        &self.buf
    }
}

impl<'a, 'b: 'a> DerefMut for OutBufferWrapper<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buf
    }
}

impl<'a> OutBuffer<'a> {
    /// Returns a new `OutBuffer` around the given slice.
    ///
    /// Starts with `pos = 0`.
    pub fn around(dst: &'a mut [u8]) -> Self {
        OutBuffer { dst, pos: 0 }
    }

    fn wrap<'b>(&'b mut self) -> OutBufferWrapper<'b, 'a> {
        OutBufferWrapper {
            buf: zstd_sys::ZSTD_outBuffer {
                dst: ptr_mut_void(self.dst),
                size: self.dst.len(),
                pos: self.pos,
            },
            parent: self,
        }
    }

    /// Returns the part of this buffer that was written to.
    pub fn as_slice<'b>(&'b self) -> &'a [u8]
    where
        'b: 'a,
    {
        let pos = self.pos;
        &self.dst[..pos]
    }
}

impl<'a, 'b> Drop for OutBufferWrapper<'a, 'b> {
    fn drop(&mut self) {
        self.parent.pos = self.buf.pos;
    }
}

struct InBufferWrapper<'a, 'b: 'a> {
    buf: zstd_sys::ZSTD_inBuffer,
    parent: &'a mut InBuffer<'b>,
}

impl<'a, 'b: 'a> Deref for InBufferWrapper<'a, 'b> {
    type Target = zstd_sys::ZSTD_inBuffer;

    fn deref(&self) -> &Self::Target {
        &self.buf
    }
}

impl<'a, 'b: 'a> DerefMut for InBufferWrapper<'a, 'b> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buf
    }
}

impl<'a> InBuffer<'a> {
    /// Returns a new `InBuffer` around the given slice.
    ///
    /// Starts with `pos = 0`.
    pub fn around(src: &'a [u8]) -> Self {
        InBuffer { src, pos: 0 }
    }

    fn wrap<'b>(&'b mut self) -> InBufferWrapper<'b, 'a> {
        InBufferWrapper {
            buf: zstd_sys::ZSTD_inBuffer {
                src: ptr_void(self.src),
                size: self.src.len(),
                pos: self.pos,
            },
            parent: self,
        }
    }
}

impl<'a, 'b> Drop for InBufferWrapper<'a, 'b> {
    fn drop(&mut self) {
        self.parent.pos = self.buf.pos;
    }
}

/// Wraps the `ZSTD_compressStream()` function.
pub fn compress_stream(
    zcs: &mut CStream,
    output: &mut OutBuffer,
    input: &mut InBuffer,
) -> SafeResult {
    let mut output = output.wrap();
    let mut input = input.wrap();
    let code = unsafe {
        zstd_sys::ZSTD_compressStream(
            zcs.0,
            ptr_mut(&mut output),
            ptr_mut(&mut input),
        )
    };
    parse_code(code)
}

/// Wraps the `ZSTD_compressStream2()` function.
pub fn compress_stream2(
    cctx: &mut CCtx,
    output: &mut OutBuffer,
    input: &mut InBuffer,
    end_op: zstd_sys::ZSTD_EndDirective,
) -> SafeResult {
    let mut output = output.wrap();
    let mut input = input.wrap();
    parse_code(unsafe {
        zstd_sys::ZSTD_compressStream2(
            cctx.0,
            ptr_mut(&mut output),
            ptr_mut(&mut input),
            end_op,
        )
    })
}

/// Wraps the `ZSTD_flushStream()` function.
pub fn flush_stream(zcs: &mut CStream, output: &mut OutBuffer) -> SafeResult {
    let mut output = output.wrap();
    let code =
        unsafe { zstd_sys::ZSTD_flushStream(zcs.0, ptr_mut(&mut output)) };
    parse_code(code)
}

/// Wraps the `ZSTD_endStream()` function.
pub fn end_stream(zcs: &mut CStream, output: &mut OutBuffer) -> SafeResult {
    let mut output = output.wrap();
    let code =
        unsafe { zstd_sys::ZSTD_endStream(zcs.0, ptr_mut(&mut output)) };
    parse_code(code)
}

/// Wraps `ZSTD_CStreamInSize()`
pub fn cstream_in_size() -> usize {
    unsafe { zstd_sys::ZSTD_CStreamInSize() }
}

/// Wraps `ZSTD_CStreamOutSize()`
pub fn cstream_out_size() -> usize {
    unsafe { zstd_sys::ZSTD_CStreamOutSize() }
}

/// A Decompression stream.
///
/// Same as `DCtx`.
pub type DStream<'a> = DCtx<'a>;

pub fn create_dstream<'a>() -> DStream<'a> {
    DCtx(unsafe { zstd_sys::ZSTD_createDStream() }, PhantomData)
}

/// Wraps the `ZSTD_initCStream()` function.
///
/// Initializes an existing `DStream` for decompression.
pub fn init_dstream(zds: &mut DStream) -> usize {
    unsafe { zstd_sys::ZSTD_initDStream(zds.0) }
}

/// Wraps the `ZSTD_decompressStream()` function.
pub fn decompress_stream(
    zds: &mut DStream,
    output: &mut OutBuffer,
    input: &mut InBuffer,
) -> SafeResult {
    let mut output = output.wrap();
    let mut input = input.wrap();
    let code = unsafe {
        zstd_sys::ZSTD_decompressStream(
            zds.0,
            ptr_mut(&mut output),
            ptr_mut(&mut input),
        )
    };
    parse_code(code)
}

/// Wraps the `ZSTD_DStreamInSize()` function.
///
/// Returns a hint for the recommended size of the input buffer for decompression.
pub fn dstream_in_size() -> usize {
    unsafe { zstd_sys::ZSTD_DStreamInSize() }
}

/// Wraps the `ZSTD_DStreamOutSize()` function.
///
/// Returns a hint for the recommended size of the output buffer for decompression.
pub fn dstream_out_size() -> usize {
    unsafe { zstd_sys::ZSTD_DStreamOutSize() }
}

/// Wraps the `ZSTD_findFrameCompressedSize()` function.
///
/// `src` should contain at least an entire frame.
pub fn find_frame_compressed_size(src: &[u8]) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_findFrameCompressedSize(ptr_void(src), src.len())
    };
    parse_code(code)
}
/// Wraps the `ZSTD_getFrameContentSize()` function.
///
/// `src` should contain at least a frame header.
pub fn get_frame_content_size(src: &[u8]) -> u64 {
    unsafe { zstd_sys::ZSTD_getFrameContentSize(ptr_void(src), src.len()) }
}

/// Wraps the `ZSTD_findDecompressedSize()` function.
///
/// `src` should be exactly a sequence of ZSTD frames.
#[cfg(feature = "experimental")]
pub fn find_decompressed_size(src: &[u8]) -> u64 {
    unsafe { zstd_sys::ZSTD_findDecompressedSize(ptr_void(src), src.len()) }
}

/// Wraps the `ZSTD_sizeofCCtx()` function.
pub fn sizeof_cctx(cctx: &CCtx) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_CCtx(cctx.0) }
}

/// Wraps the `ZSTD_sizeof_DCtx()` function.
pub fn sizeof_dctx(dctx: &DCtx) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_DCtx(dctx.0) }
}

/// Wraps the `ZSTD_sizeof_CStream()` function.
pub fn sizeof_cstream(zcs: &CStream) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_CStream(zcs.0) }
}

/// Wraps the `ZSTD_sizeof_DStream()` function.
pub fn sizeof_dstream(zds: &DStream) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_DStream(zds.0) }
}

/// Wraps the `ZSTD_sizeof_CDict()` function.
pub fn sizeof_cdict(cdict: &CDict) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_CDict(cdict.0) }
}

/// Wraps the `ZSTD_sizeof_DDict()` function.
pub fn sizeof_ddict(ddict: &DDict) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_DDict(ddict.0) }
}

/// Wraps the `ZSTD_createCDict_byReference()` function.
///
/// The dictionary will keep referencing `dict_buffer`.
#[cfg(feature = "experimental")]
pub fn create_cdict_by_reference<'a>(
    dict_buffer: &[u8],
    compression_level: CompressionLevel,
) -> CDict<'a> {
    CDict(
        unsafe {
            zstd_sys::ZSTD_createCDict_byReference(
                ptr_void(dict_buffer),
                dict_buffer.len(),
                compression_level,
            )
        },
        PhantomData,
    )
}

/// Wraps the `ZSTD_isFrame()` function.
#[cfg(feature = "experimental")]
pub fn is_frame(buffer: &[u8]) -> u32 {
    unsafe { zstd_sys::ZSTD_isFrame(ptr_void(buffer), buffer.len()) as u32 }
}

/// Wraps the `ZSTD_createDDict_byReference()` function.
///
/// The dictionary will keep referencing `dict_buffer`.
#[cfg(feature = "experimental")]
pub fn create_ddict_by_reference(dict_buffer: &[u8]) -> DDict {
    DDict(
        unsafe {
            zstd_sys::ZSTD_createDDict_byReference(
                ptr_void(dict_buffer),
                dict_buffer.len(),
            )
        },
        PhantomData,
    )
}

/// Wraps the `ZSTD_getDictID_fromDict()` function.
pub fn get_dict_id_from_dict(dict: &[u8]) -> u32 {
    unsafe {
        zstd_sys::ZSTD_getDictID_fromDict(ptr_void(dict), dict.len()) as u32
    }
}

/// Wraps the `ZSTD_getDictID_fromDDict()` function.
pub fn get_dict_id_from_ddict(ddict: &DDict) -> u32 {
    unsafe { zstd_sys::ZSTD_getDictID_fromDDict(ddict.0) as u32 }
}

/// Wraps the `ZSTD_getDictID_fromFrame()` function.
pub fn get_dict_id_from_frame(src: &[u8]) -> u32 {
    unsafe {
        zstd_sys::ZSTD_getDictID_fromFrame(ptr_void(src), src.len()) as u32
    }
}

/// Wraps the `ZSTD_initCStream_srcSize()` function.
#[cfg(feature = "experimental")]
pub fn init_cstream_src_size(
    zcs: &mut CStream,
    compression_level: CompressionLevel,
    pledged_src_size: u64,
) -> usize {
    unsafe {
        zstd_sys::ZSTD_initCStream_srcSize(
            zcs.0,
            compression_level as c_int,
            pledged_src_size as c_ulonglong,
        )
    }
}

/// Wraps the `ZSTD_initCStream_usingDict()` function.
#[cfg(feature = "experimental")]
pub fn init_cstream_using_dict(
    zcs: &mut CStream,
    dict: &[u8],
    compression_level: CompressionLevel,
) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_initCStream_usingDict(
            zcs.0,
            ptr_void(dict),
            dict.len(),
            compression_level,
        )
    };
    parse_code(code)
}

/// Wraps the `ZSTD_initCStream_usingCDict()` function.
#[cfg(feature = "experimental")]
pub fn init_cstream_using_cdict(
    zcs: &mut CStream,
    cdict: &CDict,
) -> SafeResult {
    let code =
        unsafe { zstd_sys::ZSTD_initCStream_usingCDict(zcs.0, cdict.0) };
    parse_code(code)
}

/// Wraps the `ZSTD_CCtx_loadDictionary()` function.
pub fn cctx_load_dictionary(cctx: &mut CCtx, dict: &[u8]) -> SafeResult {
    parse_code(unsafe {
        zstd_sys::ZSTD_CCtx_loadDictionary(cctx.0, ptr_void(dict), dict.len())
    })
}

/// Wraps the `ZSTD_CCtx_refCDict()` function.
///
/// Dictionary must outlive the context.
pub fn cctx_ref_cdict<'a, 'b>(
    cctx: &mut CCtx<'a>,
    cdict: &'b CDict<'b>,
) -> SafeResult
where
    'b: 'a,
{
    parse_code(unsafe { zstd_sys::ZSTD_CCtx_refCDict(cctx.0, cdict.0) })
}

/// Wraps the `ZSTD_CCtx_refPrefix()` function.
///
/// Dictionary must outlive the prefix.
pub fn cctx_ref_prefix<'a, 'b>(
    cctx: &mut CCtx<'a>,
    prefix: &'b [u8],
) -> SafeResult
where
    'b: 'a,
{
    parse_code(unsafe {
        zstd_sys::ZSTD_CCtx_refPrefix(cctx.0, ptr_void(prefix), prefix.len())
    })
}

/// Wraps the `ZSTD_DCtx_loadDictionary()` function.
pub fn dctx_load_dictionary(dctx: &mut DCtx<'_>, dict: &[u8]) -> SafeResult {
    parse_code(unsafe {
        zstd_sys::ZSTD_DCtx_loadDictionary(dctx.0, ptr_void(dict), dict.len())
    })
}

/// Wraps the `ZSTD_DCtx_refDDict()` function.
pub fn dctx_ref_ddict<'a, 'b>(
    dctx: &mut DCtx<'a>,
    ddict: &'b DDict<'b>,
) -> SafeResult
where
    'b: 'a,
{
    parse_code(unsafe { zstd_sys::ZSTD_DCtx_refDDict(dctx.0, ddict.0) })
}

/// Wraps the `ZSTD_DCtx_refPrefix()` function.
pub fn dctx_ref_prefix<'a, 'b>(
    dctx: &mut DCtx<'a>,
    prefix: &'b [u8],
) -> SafeResult
where
    'b: 'a,
{
    parse_code(unsafe {
        zstd_sys::ZSTD_DCtx_refPrefix(dctx.0, ptr_void(prefix), prefix.len())
    })
}

/// Wraps the `ZSTD_CCtx_reset()` function.
pub fn cctx_reset(cctx: &mut CCtx, reset: ResetDirective) -> SafeResult {
    parse_code(unsafe { zstd_sys::ZSTD_CCtx_reset(cctx.0, reset) })
}

/// Wraps the `ZSTD_DCtx_reset()` function.
pub fn dctx_reset(dctx: &mut DCtx, reset: ResetDirective) -> SafeResult {
    parse_code(unsafe { zstd_sys::ZSTD_DCtx_reset(dctx.0, reset) })
}

/// Wraps the `ZSTD_resetCStream()` function.
#[cfg(feature = "experimental")]
pub fn reset_cstream(zcs: &mut CStream, pledged_src_size: u64) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_resetCStream(zcs.0, pledged_src_size as c_ulonglong)
    };
    parse_code(code)
}

/// Wraps the `ZSTD_initDStream_usingDict()` function.
#[cfg(feature = "experimental")]
pub fn init_dstream_using_dict(zds: &mut DStream, dict: &[u8]) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_initDStream_usingDict(zds.0, ptr_void(dict), dict.len())
    };
    parse_code(code)
}

/// Wraps the `ZSTD_initDStream_usingDDict()` function.
#[cfg(feature = "experimental")]
pub fn init_dstream_using_ddict(
    zds: &mut DStream,
    ddict: &DDict,
) -> SafeResult {
    let code =
        unsafe { zstd_sys::ZSTD_initDStream_usingDDict(zds.0, ddict.0) };
    parse_code(code)
}

/// Wraps the `ZSTD_resetDStream()` function.
#[cfg(feature = "experimental")]
pub fn reset_dstream(zds: &mut DStream) -> SafeResult {
    let code = unsafe { zstd_sys::ZSTD_resetDStream(zds.0) };
    parse_code(code)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FrameFormat {
    /// Regular zstd format.
    One,

    /// Skip the 4 bytes identifying the content as zstd-compressed data.
    Magicless,
}

/// A compression parameter.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CParameter {
    #[cfg(feature = "experimental")]
    Format(FrameFormat),

    CompressionLevel(CompressionLevel),

    WindowLog(u32),

    HashLog(u32),

    ChainLog(u32),

    SearchLog(u32),

    MinMatch(u32),

    TargetLength(u32),

    Strategy(Strategy),

    EnableLongDistanceMatching(bool),

    LdmHashLog(u32),

    LdmMinMatch(u32),

    LdmBucketSizeLog(u32),

    LdmHashRateLog(u32),

    ContentSizeFlag(bool),

    ChecksumFlag(bool),

    DictIdFlag(bool),

    NbWorkers(u32),

    JobSize(u32),

    OverlapSizeLog(u32),
}

/// A decompression parameter.
pub enum DParameter {
    WindowLogMax(u32),

    /// See `FrameFormat`.
    #[cfg(feature = "experimental")]
    Format(FrameFormat),
}

/// Wraps the `ZSTD_DCtx_setParameter()` function.
pub fn dctx_set_parameter(dctx: &mut DCtx, param: DParameter) -> SafeResult {
    #[cfg(feature = "experimental")]
    use zstd_sys::ZSTD_dParameter::ZSTD_d_experimentalParam1 as ZSTD_d_format;
    #[cfg(feature = "experimental")]
    use zstd_sys::ZSTD_format_e;

    use zstd_sys::ZSTD_dParameter::*;
    use DParameter::*;

    let (param, value) = match param {
        #[cfg(feature = "experimental")]
        Format(FrameFormat::One) => {
            (ZSTD_d_format, ZSTD_format_e::ZSTD_f_zstd1 as c_int)
        }
        #[cfg(feature = "experimental")]
        Format(FrameFormat::Magicless) => (
            ZSTD_d_format,
            ZSTD_format_e::ZSTD_f_zstd1_magicless as c_int,
        ),

        WindowLogMax(value) => (ZSTD_d_windowLogMax, value as c_int),
    };

    parse_code(unsafe {
        zstd_sys::ZSTD_DCtx_setParameter(dctx.0, param, value)
    })
}

/// Wraps the `ZSTD_CCtx_setParameter()` function.
pub fn cctx_set_parameter(cctx: &mut CCtx, param: CParameter) -> SafeResult {
    // TODO: Until bindgen properly generates a binding for this, we'll need to do it here.
    #[cfg(feature = "experimental")]
    use zstd_sys::ZSTD_cParameter::ZSTD_c_experimentalParam2 as ZSTD_c_format;
    #[cfg(feature = "experimental")]
    use zstd_sys::ZSTD_format_e;

    use zstd_sys::ZSTD_cParameter::*;
    use CParameter::*;

    let (param, value) = match param {
        #[cfg(feature = "experimental")]
        Format(FrameFormat::One) => {
            (ZSTD_c_format, ZSTD_format_e::ZSTD_f_zstd1 as c_int)
        }
        #[cfg(feature = "experimental")]
        Format(FrameFormat::Magicless) => (
            ZSTD_c_format,
            ZSTD_format_e::ZSTD_f_zstd1_magicless as c_int,
        ),
        CompressionLevel(level) => (ZSTD_c_compressionLevel, level),
        WindowLog(value) => (ZSTD_c_windowLog, value as c_int),
        HashLog(value) => (ZSTD_c_hashLog, value as c_int),
        ChainLog(value) => (ZSTD_c_chainLog, value as c_int),
        SearchLog(value) => (ZSTD_c_searchLog, value as c_int),
        MinMatch(value) => (ZSTD_c_minMatch, value as c_int),
        TargetLength(value) => (ZSTD_c_targetLength, value as c_int),
        Strategy(strategy) => (ZSTD_c_strategy, strategy as c_int),
        EnableLongDistanceMatching(flag) => {
            (ZSTD_c_enableLongDistanceMatching, flag as c_int)
        }
        LdmHashLog(value) => (ZSTD_c_ldmHashLog, value as c_int),
        LdmMinMatch(value) => (ZSTD_c_ldmMinMatch, value as c_int),
        LdmBucketSizeLog(value) => (ZSTD_c_ldmBucketSizeLog, value as c_int),
        LdmHashRateLog(value) => (ZSTD_c_ldmHashRateLog, value as c_int),
        ContentSizeFlag(flag) => (ZSTD_c_contentSizeFlag, flag as c_int),
        ChecksumFlag(flag) => (ZSTD_c_checksumFlag, flag as c_int),
        DictIdFlag(flag) => (ZSTD_c_dictIDFlag, flag as c_int),

        NbWorkers(value) => (ZSTD_c_nbWorkers, value as c_int),

        JobSize(value) => (ZSTD_c_jobSize, value as c_int),

        OverlapSizeLog(value) => (ZSTD_c_overlapLog, value as c_int),
    };

    parse_code(unsafe {
        zstd_sys::ZSTD_CCtx_setParameter(cctx.0, param, value)
    })
}

/// Wraps the `ZSTD_CCtx_setPledgedSrcSize()` function.
pub fn cctx_set_pledged_src_size(
    cctx: &mut CCtx,
    pledged_src_size: u64,
) -> SafeResult {
    parse_code(unsafe {
        zstd_sys::ZSTD_CCtx_setPledgedSrcSize(
            cctx.0,
            pledged_src_size as c_ulonglong,
        )
    })
}

/// Wraps thge `ZDICT_trainFromBuffer()` function.
pub fn train_from_buffer(
    dict_buffer: &mut [u8],
    samples_buffer: &[u8],
    samples_sizes: &[usize],
) -> SafeResult {
    assert_eq!(samples_buffer.len(), samples_sizes.iter().sum());
    parse_code(unsafe {
        zstd_sys::ZDICT_trainFromBuffer(
            ptr_mut_void(dict_buffer),
            dict_buffer.len(),
            ptr_void(samples_buffer),
            samples_sizes.as_ptr(),
            samples_sizes.len() as u32,
        )
    })
}

/// Wraps the `ZSTD_getDictID_fromDict()` function.
pub fn get_dict_id(dict_buffer: &[u8]) -> Option<u32> {
    let id = unsafe {
        zstd_sys::ZDICT_getDictID(ptr_void(dict_buffer), dict_buffer.len())
    };
    if id > 0 {
        Some(id)
    } else {
        None
    }
}

/// Wraps the `ZSTD_getBlockSize()` function.
#[cfg(feature = "experimental")]
pub fn get_block_size(cctx: &mut CCtx) -> usize {
    unsafe { zstd_sys::ZSTD_getBlockSize(cctx.0) }
}

/// Wraps the `ZSTD_compressBlock()` function.
#[cfg(feature = "experimental")]
pub fn compress_block(
    cctx: &mut CCtx,
    dst: &mut [u8],
    src: &[u8],
) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_compressBlock(
            cctx.0,
            ptr_mut_void(dst),
            dst.len(),
            ptr_void(src),
            src.len(),
        )
    };
    parse_code(code)
}

/// Wraps the `ZSTD_decompressBlock()` function.
#[cfg(feature = "experimental")]
pub fn decompress_block(dctx: &mut DCtx, dst: &mut [u8], src: &[u8]) -> usize {
    unsafe {
        zstd_sys::ZSTD_decompressBlock(
            dctx.0,
            ptr_mut_void(dst),
            dst.len(),
            ptr_void(src),
            src.len(),
        )
    }
}

/// Wraps the `ZSTD_insertBlock()` function.
#[cfg(feature = "experimental")]
pub fn insert_block(dctx: &mut DCtx, block: &[u8]) -> usize {
    unsafe { zstd_sys::ZSTD_insertBlock(dctx.0, ptr_void(block), block.len()) }
}
