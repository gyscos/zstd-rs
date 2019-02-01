#![no_std]
//! Minimal safe wrapper around zstd-sys.
//!
//! This crates provides a minimal translation of the zstd-sys methods.
//! For a more comfortable high-level library, see the [zstd] crate.
//!
//! [zstd]: https://crates.io/crates/zstd
//!
//! # Introduction
//!
//! zstd, short for Zstandard, is a fast lossless compression algorithm, targeting real-time compression scenarios
//! at zlib-level and better compression ratios. The zstd compression library provides in-memory compression and
//! decompression functions. The library supports compression levels from 1 up to ZSTD_maxCLevel() which is 22.
//! Levels >= 20, labeled `--ultra`, should be used with caution, as they require more memory.
//!
//! Compression can be done in:
//!
//! * a single step (described as Simple API)
//! * a single step, reusing a context (described as Explicit memory management)
//! * unbounded multiple steps (described as Streaming compression)
//!
//! The compression ratio achievable on small data can be highly improved using compression with a dictionary in:
//!
//! * a single step (described as Simple dictionary API)
//! * a single step, reusing a dictionary (described as Fast dictionary API)
//!
//! Advanced experimental functions can be accessed using #define ZSTD_STATIC_LINKING_ONLY before including zstd.h.
//! These APIs shall never be used with a dynamic library.
//! They are not "stable", their definition may change in the future. Only static linking is allowed.

extern crate libc;
extern crate zstd_sys;

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
use std::os::raw::{c_int, c_ulonglong, c_void};

#[cfg(not(feature = "std"))]
use libc::{c_int, c_ulonglong, c_void};

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
pub const CLEVEL_DEFAULT: i32 = zstd_sys::ZSTD_CLEVEL_DEFAULT as i32;
pub const CONTENTSIZE_UNKNOWN: u64 = zstd_sys::ZSTD_CONTENTSIZE_UNKNOWN as u64;
pub const CONTENTSIZE_ERROR: u64 = zstd_sys::ZSTD_CONTENTSIZE_ERROR as u64;
#[cfg(feature = "experimental")]
pub const MAGICNUMBER: u32 = zstd_sys::ZSTD_MAGICNUMBER;
#[cfg(feature = "experimental")]
pub const MAGIC_DICTIONARY: u32 = zstd_sys::ZSTD_MAGIC_DICTIONARY;
#[cfg(feature = "experimental")]
pub const MAGIC_SKIPPABLE_START: u32 = zstd_sys::ZSTD_MAGIC_SKIPPABLE_START;
#[cfg(feature = "experimental")]
pub const BLOCKSIZELOG_MAX: u32 = zstd_sys::ZSTD_BLOCKSIZELOG_MAX;
#[cfg(feature = "experimental")]
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
// pub const SEARCHLENGTH_MAX: u32 = zstd_sys::ZSTD_SEARCHLENGTH_MAX;
// pub const SEARCHLENGTH_MIN: u32 = zstd_sys::ZSTD_SEARCHLENGTH_MIN;
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
#[cfg(feature = "experimental")]
pub const FRAMEHEADERSIZE_PREFIX: u32 = zstd_sys::ZSTD_FRAMEHEADERSIZE_PREFIX;
#[cfg(feature = "experimental")]
pub const FRAMEHEADERSIZE_MIN: u32 = zstd_sys::ZSTD_FRAMEHEADERSIZE_MIN;
#[cfg(feature = "experimental")]
pub const FRAMEHEADERSIZE_MAX: u32 = zstd_sys::ZSTD_FRAMEHEADERSIZE_MAX;
// pub const JOBSIZE_MIN: u32 = zstd_sys::ZSTDMT_JOBSIZE_MIN;

type SafeResult = Result<usize, usize>;

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

/// `ZSTD_compress`
///
/// Compresses `src` content as a single zstd compressed frame into already allocated `dst`.
///
/// Hint : compression runs faster if `dstCapacity` >=  `ZSTD_compressBound(srcSize)`.
///
/// Returns the compressed size written into `dst` (<= `dstCapacity),
/// or an error code if it fails (which can be tested using ZSTD_isError()).
pub fn compress(
    dst: &mut [u8],
    src: &[u8],
    compression_level: i32,
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

/// `ZSTD_decompress`
///
/// `compressedSize` : must be the _exact_ size of some number of compressed and/or skippable frames.
///
/// `dstCapacity` is an upper bound of originalSize.
///
/// If user cannot imply a maximum upper bound, it's better to use streaming mode to decompress data.
///
/// Returns the number of bytes decompressed into `dst` (<= `dstCapacity`),
/// or an errorCode if it fails (which can be tested using ZSTD_isError()).
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

/// `ZSTD_getDecompressedSize()`
///
/// NOTE: This function is planned to be obsolete, in favour of ZSTD_getFrameContentSize.
///
/// ZSTD_getFrameContentSize functions the same way, returning the decompressed size of a single
/// frame, but distinguishes empty frames from frames with an unknown size, or errors.
///
/// Additionally, ZSTD_findDecompressedSize can be used instead.  It can handle multiple
/// concatenated frames in one buffer, and so is more general.
///
/// As a result however, it requires more computation and entire frames to be passed to it,
/// as opposed to ZSTD_getFrameContentSize which requires only a single frame's header.
///
/// `src` is the start of a zstd compressed frame.
///
/// Returns content size to be decompressed, as a 64-bits value _if known_, 0 otherwise.
///
///  note 1 : decompressed size is an optional field, that may not be present, especially in streaming mode.
///           When `return==0`, data to decompress could be any size.
///           In which case, it's necessary to use streaming mode to decompress data.
///           Optionally, application can still use ZSTD_decompress() while relying on implied limits.
///           (For example, data may be necessarily cut into blocks <= 16 KB).
///
///  note 2 : decompressed size is always present when compression is done with ZSTD_compress()
///
///  note 3 : decompressed size can be very large (64-bits value),
///           potentially larger than what local system can handle as a single memory segment.
///           In which case, it's necessary to use streaming mode to decompress data.
///
///  note 4 : If source is untrusted, decompressed size could be wrong or intentionally modified.
///           Always ensure result fits within application's authorized limits.
///           Each application can set its own limits.
///
///  note 5 : when `return==0`, if precise failure cause is needed, use ZSTD_getFrameParams() to know more.
pub fn get_decompressed_size(src: &[u8]) -> u64 {
    unsafe {
        zstd_sys::ZSTD_getDecompressedSize(ptr_void(src), src.len()) as u64
    }
}

pub fn max_clevel() -> i32 {
    unsafe { zstd_sys::ZSTD_maxCLevel() as i32 }
}

pub fn compress_bound(src_size: usize) -> usize {
    unsafe { zstd_sys::ZSTD_compressBound(src_size) }
}

pub struct CCtx(*mut zstd_sys::ZSTD_CCtx);

impl Default for CCtx {
    fn default() -> Self {
        create_cctx()
    }
}

pub fn create_cctx() -> CCtx {
    CCtx(unsafe { zstd_sys::ZSTD_createCCtx() })
}

impl Drop for CCtx {
    fn drop(&mut self) {
        unsafe {
            zstd_sys::ZSTD_freeCCtx(self.0);
        }
    }
}

unsafe impl Send for CCtx {}
// CCtx can't be shared across threads, so it does not implement Sync.

#[cfg(not(feature = "std"))]
pub fn get_error_name(code: usize) -> &'static str {
    unsafe {
        // We are getting a *const char from zstd
        let name = zstd_sys::ZSTD_getErrorName(code);

        // To be safe, we need to compute right now its length
        let len = libc::strlen(name);

        // Cast it to a slice
        let slice = core::slice::from_raw_parts(name as *mut u8, len);
        // And hope it's still text.
        str::from_utf8(slice).expect("bad error message from zstd")
    }
}

#[cfg(feature = "std")]
pub fn get_error_name(code: usize) -> &'static str {
    unsafe {
        // We are getting a *const char from zstd
        let name = zstd_sys::ZSTD_getErrorName(code);

        std::ffi::CStr::from_ptr(name)
            .to_str()
            .expect("bad error message from zstd")
    }
}

/// `ZSTD_compressCCtx()`
///
/// Same as `ZSTD_compress()`, requires an allocated `ZSTD_CCtx` (see `ZSTD_createCCtx()`).
pub fn compress_cctx(
    ctx: &mut CCtx,
    dst: &mut [u8],
    src: &[u8],
    compression_level: i32,
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

pub struct DCtx(*mut zstd_sys::ZSTD_DCtx);

impl Default for DCtx {
    fn default() -> Self {
        create_dctx()
    }
}

pub fn create_dctx() -> DCtx {
    DCtx(unsafe { zstd_sys::ZSTD_createDCtx() })
}

impl Drop for DCtx {
    fn drop(&mut self) {
        unsafe {
            zstd_sys::ZSTD_freeDCtx(self.0);
        }
    }
}

unsafe impl Send for DCtx {}
// DCtx can't be shared across threads, so it does not implement Sync.

/// `ZSTD_decompressDCtx()`
///
/// Same as ZSTD_decompress(), requires an allocated ZSTD_DCtx (see ZSTD_createDCtx()).
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

/// `ZSTD_compress_usingDict()`
///
/// Compression using a predefined Dictionary (see dictBuilder/zdict.h).
///
/// Note : This function loads the dictionary, resulting in significant startup delay.
///
/// Note : When `dict == NULL || dictSize < 8` no dictionary is used.
pub fn compress_using_dict(
    ctx: &mut CCtx,
    dst: &mut [u8],
    src: &[u8],
    dict: &[u8],
    compression_level: i32,
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

/// `ZSTD_decompress_usingDict()`
///
/// Decompression using a predefined Dictionary (see dictBuilder/zdict.h).
///
/// Dictionary must be identical to the one used during compression.
///
/// Note : This function loads the dictionary, resulting in significant startup delay.
///
/// Note : When `dict == NULL || dictSize < 8` no dictionary is used.
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

pub struct CDict<'a>(*mut zstd_sys::ZSTD_CDict, PhantomData<&'a ()>);

/// `ZSTD_createCDict()`
///
/// When compressing multiple messages / blocks with the same dictionary, it's recommended to load it just once.
///
/// ZSTD_createCDict() will create a digested dictionary, ready to start future compression operations without startup delay.
///
/// ZSTD_CDict can be created once and used by multiple threads concurrently, as its usage is read-only.
///
/// `dictBuffer` can be released after ZSTD_CDict creation, as its content is copied within CDict
pub fn create_cdict(
    dict_buffer: &[u8],
    compression_level: i32,
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

/// `ZSTD_compress_usingCDict()`
///
/// Compression using a digested Dictionary.
///
/// Faster startup than ZSTD_compress_usingDict(), recommended when same dictionary is used multiple times.
///
/// Note that compression level is decided during dictionary creation.
///
/// Frame parameters are hardcoded (dictID=yes, contentSize=yes, checksum=no)
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

/// `ZSTD_createDDict()`
///
/// Create a digested dictionary, ready to start decompression operation without startup delay.
///
/// dictBuffer can be released after DDict creation, as its content is copied inside DDict
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

/// `ZSTD_decompress_usingDDict()`
///
/// Decompression using a digested Dictionary.
///
/// Faster startup than ZSTD_decompress_usingDict(), recommended when same dictionary is used multiple times.
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

pub type CStream = CCtx;

// CStream can't be shared across threads, so it does not implement Sync.

pub fn create_cstream() -> CStream {
    CCtx(unsafe { zstd_sys::ZSTD_createCStream() })
}

pub fn init_cstream(zcs: &mut CStream, compression_level: i32) -> usize {
    unsafe { zstd_sys::ZSTD_initCStream(zcs.0, compression_level) }
}

#[derive(Debug)]
pub struct InBuffer<'a> {
    pub src: &'a [u8],
    pub pos: usize,
}

#[derive(Debug)]
pub struct OutBuffer<'a> {
    pub dst: &'a mut [u8],
    pub pos: usize,
}

fn ptr_mut<B>(ptr_void: &mut B) -> *mut B {
    ptr_void as *mut B
}

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

pub fn flush_stream(zcs: &mut CStream, output: &mut OutBuffer) -> SafeResult {
    let mut output = output.wrap();
    let code =
        unsafe { zstd_sys::ZSTD_flushStream(zcs.0, ptr_mut(&mut output)) };
    parse_code(code)
}

pub fn end_stream(zcs: &mut CStream, output: &mut OutBuffer) -> SafeResult {
    let mut output = output.wrap();
    let code =
        unsafe { zstd_sys::ZSTD_endStream(zcs.0, ptr_mut(&mut output)) };
    parse_code(code)
}

pub fn cstream_in_size() -> usize {
    unsafe { zstd_sys::ZSTD_CStreamInSize() }
}

pub fn cstream_out_size() -> usize {
    unsafe { zstd_sys::ZSTD_CStreamOutSize() }
}

pub type DStream = DCtx;

pub fn create_dstream() -> DStream {
    DCtx(unsafe { zstd_sys::ZSTD_createDStream() })
}

pub fn init_dstream(zds: &mut DStream) -> usize {
    unsafe { zstd_sys::ZSTD_initDStream(zds.0) }
}

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

pub fn dstream_in_size() -> usize {
    unsafe { zstd_sys::ZSTD_DStreamInSize() }
}

pub fn dstream_out_size() -> usize {
    unsafe { zstd_sys::ZSTD_DStreamOutSize() }
}

/// `ZSTD_findFrameCompressedSize()`
///
/// `src` should point to the start of a ZSTD encoded frame or skippable frame
///
/// `srcSize` must be at least as large as the frame
///
/// Returns the compressed size of the frame pointed to by `src`, suitable to pass to
/// `ZSTD_decompress` or similar, or an error code if given invalid input.
#[cfg(feature = "experimental")]
pub fn find_frame_compressed_size(src: &[u8]) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_findFrameCompressedSize(ptr_void(src), src.len())
    };
    parse_code(code)
}
/// `ZSTD_getFrameContentSize()`
///
/// `src` should point to the start of a ZSTD encoded frame
///
/// `srcSize` must be at least as large as the frame header.  A value greater than or equal
///     to `ZSTD_frameHeaderSize_max` is guaranteed to be large enough in all cases.
///
/// Returns the decompressed size of the frame pointed to be `src` if known, otherwise:
///
/// * ZSTD_CONTENTSIZE_UNKNOWN if the size cannot be determined
/// * ZSTD_CONTENTSIZE_ERROR if an error occurred (e.g. invalid magic number, srcSize too small)
pub fn get_frame_content_size(src: &[u8]) -> u64 {
    unsafe { zstd_sys::ZSTD_getFrameContentSize(ptr_void(src), src.len()) }
}

/// `ZSTD_findDecompressedSize()`
///
/// `src` should point the start of a series of ZSTD encoded and/or skippable frames
///
/// `srcSize` must be the _exact_ size of this series
///     (i.e. there should be a frame boundary exactly `srcSize` bytes after `src`)
///
/// Returns the decompressed size of all data in the contained frames, as a 64-bit value _if known_
///
/// * if the decompressed size cannot be determined: ZSTD_CONTENTSIZE_UNKNOWN
/// * if an error occurred: ZSTD_CONTENTSIZE_ERROR
///
///
/// note 1 : decompressed size is an optional field, that may not be present, especially in streaming mode.
///          When `return==ZSTD_CONTENTSIZE_UNKNOWN`, data to decompress could be any size.
///          In which case, it's necessary to use streaming mode to decompress data.
///          Optionally, application can still use ZSTD_decompress() while relying on implied limits.
///          (For example, data may be necessarily cut into blocks <= 16 KB).
///
/// note 2 : decompressed size is always present when compression is done with ZSTD_compress()
///
/// note 3 : decompressed size can be very large (64-bits value),
///          potentially larger than what local system can handle as a single memory segment.
///          In which case, it's necessary to use streaming mode to decompress data.
///
/// note 4 : If source is untrusted, decompressed size could be wrong or intentionally modified.
///          Always ensure result fits within application's authorized limits.
///          Each application can set its own limits.
///
/// note 5 : ZSTD_findDecompressedSize handles multiple frames, and so it must traverse the input to
///          read each contained frame header.  This is efficient as most of the data is skipped,
///          however it does mean that all frame data must be present and valid.
#[cfg(feature = "experimental")]
pub fn find_decompressed_size(src: &[u8]) -> u64 {
    unsafe { zstd_sys::ZSTD_findDecompressedSize(ptr_void(src), src.len()) }
}

/// `ZSTD_sizeofCCtx()`
///
/// Gives the amount of memory used by a given ZSTD_CCtx
#[cfg(feature = "experimental")]
pub fn sizeof_cctx(cctx: &CCtx) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_CCtx(cctx.0) }
}

/// `ZSTD_sizeof_DCtx()`
///
/// Gives the amount of memory used by a given ZSTD_DCtx
#[cfg(feature = "experimental")]
pub fn sizeof_dctx(dctx: &DCtx) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_DCtx(dctx.0) }
}

#[cfg(feature = "experimental")]
pub fn sizeof_cstream(zcs: &CStream) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_CStream(zcs.0) }
}
#[cfg(feature = "experimental")]
pub fn sizeof_dstream(zds: &DStream) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_DStream(zds.0) }
}
/// `ZSTD_sizeof_CDict()`
///
/// Gives the amount of memory used by a given ZSTD_sizeof_CDict
#[cfg(feature = "experimental")]
pub fn sizeof_cdict(cdict: &CDict) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_CDict(cdict.0) }
}
/// `ZSTD_sizeof_DDict()`
///
/// Gives the amount of memory used by a given ZSTD_DDict
#[cfg(feature = "experimental")]
pub fn sizeof_ddict(ddict: &DDict) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_DDict(ddict.0) }
}
/// `ZSTD_createCDict_byReference()`
///
/// Create a digested dictionary for compression
///
/// Dictionary content is simply referenced, and therefore stays in dictBuffer.
///
/// It is important that dictBuffer outlives CDict, it must remain read accessible throughout the lifetime of CDict
#[cfg(feature = "experimental")]
pub fn create_cdict_by_reference(
    dict_buffer: &[u8],
    compression_level: i32,
) -> CDict {
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

/// `ZSTD_isFrame()`
///
/// Tells if the content of `buffer` starts with a valid Frame Identifier.
///
/// Note : Frame Identifier is 4 bytes. If `size < 4`, @return will always be 0.
///
/// Note 2 : Legacy Frame Identifiers are considered valid only if Legacy Support is enabled.
///
/// Note 3 : Skippable Frame Identifiers are considered valid.
#[cfg(feature = "experimental")]
pub fn is_frame(buffer: &[u8]) -> u32 {
    unsafe { zstd_sys::ZSTD_isFrame(ptr_void(buffer), buffer.len()) as u32 }
}

/// `ZSTD_createDDict_byReference()`
///
/// Create a digested dictionary, ready to start decompression operation without startup delay.
///
/// Dictionary content is simply referenced, and therefore stays in dictBuffer.
///
/// It is important that dictBuffer outlives DDict, it must remain read accessible throughout the lifetime of DDict
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
/// `ZSTD_getDictID_fromDict()`
///
/// Provides the dictID stored within dictionary.
///
/// if @return == 0, the dictionary is not conformant with Zstandard specification.
///
/// It can still be loaded, but as a content-only dictionary.
#[cfg(feature = "experimental")]
pub fn get_dict_id_from_dict(dict: &[u8]) -> u32 {
    unsafe {
        zstd_sys::ZSTD_getDictID_fromDict(ptr_void(dict), dict.len()) as u32
    }
}
/// `ZSTD_getDictID_fromDDict()`
///
/// Provides the dictID of the dictionary loaded into `ddict`.
///
/// If @return == 0, the dictionary is not conformant to Zstandard specification, or empty.
///
/// Non-conformant dictionaries can still be loaded, but as content-only dictionaries.
#[cfg(feature = "experimental")]
pub fn get_dict_id_from_ddict(ddict: &DDict) -> u32 {
    unsafe { zstd_sys::ZSTD_getDictID_fromDDict(ddict.0) as u32 }
}
/// `ZSTD_getDictID_fromFrame()`
///
/// Provides the dictID required to decompressed the frame stored within `src`.
///
/// If @return == 0, the dictID could not be decoded.
///
/// This could for one of the following reasons :
///
/// * The frame does not require a dictionary to be decoded (most common case).
/// * The frame was built with dictID intentionally removed. Whatever dictionary is necessary is a hidden information.
///   Note : this use case also happens when using a non-conformant dictionary.
/// * `srcSize` is too small, and as a result, the frame header could not be decoded (only possible if `srcSize < ZSTD_FRAMEHEADERSIZE_MAX`).
/// * This is not a Zstandard frame.
///
/// When identifying the exact failure cause, it's possible to use ZSTD_getFrameParams(), which will provide a more precise error code.
#[cfg(feature = "experimental")]
pub fn get_dict_id_from_frame(src: &[u8]) -> u32 {
    unsafe {
        zstd_sys::ZSTD_getDictID_fromFrame(ptr_void(src), src.len()) as u32
    }
}

#[cfg(feature = "experimental")]
pub fn init_cstream_src_size(
    zcs: &mut CStream,
    compression_level: i32,
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

#[cfg(feature = "experimental")]
pub fn init_cstream_using_dict(
    zcs: &mut CStream,
    dict: &[u8],
    compression_level: i32,
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

#[cfg(feature = "experimental")]
pub fn init_cstream_using_cdict(
    zcs: &mut CStream,
    cdict: &CDict,
) -> SafeResult {
    let code =
        unsafe { zstd_sys::ZSTD_initCStream_usingCDict(zcs.0, cdict.0) };
    parse_code(code)
}

/// `ZSTD_resetCStream()`
///
/// Start a new compression job, using same parameters from previous job.
///
/// This is typically useful to skip dictionary loading stage, since it will re-use it in-place.
///
/// Note that zcs must be init at least once before using ZSTD_resetCStream().
///
/// If pledgedSrcSize is not known at reset time, use macro ZSTD_CONTENTSIZE_UNKNOWN.
///
/// If pledgedSrcSize > 0, its value must be correct, as it will be written in header, and controlled at the end.
///
/// For the time being, pledgedSrcSize==0 is interpreted as "srcSize unknown" for compatibility with older programs,
/// but it will change to mean "empty" in future version, so use macro ZSTD_CONTENTSIZE_UNKNOWN instead.
///
/// Returns 0, or an error code (which can be tested using ZSTD_isError())
#[cfg(feature = "experimental")]
pub fn reset_cstream(zcs: &mut CStream, pledged_src_size: u64) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_resetCStream(zcs.0, pledged_src_size as c_ulonglong)
    };
    parse_code(code)
}
#[cfg(feature = "experimental")]
pub fn init_dstream_using_dict(zds: &mut DStream, dict: &[u8]) -> SafeResult {
    let code = unsafe {
        zstd_sys::ZSTD_initDStream_usingDict(zds.0, ptr_void(dict), dict.len())
    };
    parse_code(code)
}
#[cfg(feature = "experimental")]
pub fn init_dstream_using_ddict(
    zds: &mut DStream,
    ddict: &DDict,
) -> SafeResult {
    let code =
        unsafe { zstd_sys::ZSTD_initDStream_usingDDict(zds.0, ddict.0) };
    parse_code(code)
}
#[cfg(feature = "experimental")]
pub fn reset_dstream(zds: &mut DStream) -> SafeResult {
    let code = unsafe { zstd_sys::ZSTD_resetDStream(zds.0) };
    parse_code(code)
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FrameFormat {
    /// zstd frame format, specified in zstd_compression_format.md (default)
    One,

    /// Variant of zstd frame format, without initial 4-bytes magic number.
    /// Useful to save 4 bytes per generated frame.
    /// Decoder cannot recognise automatically this format, requiring instructions.
    Magicless,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CParameter {
    /// See `FrameFormat`.
    Format(FrameFormat),

    /// Update all compression parameters according to pre-defined cLevel table.
    ///
    /// Default level is ZSTD_CLEVEL_DEFAULT==3.
    ///
    /// Special: value 0 means "do not change cLevel".
    CompressionLevel(i32),

    /// Maximum allowed back-reference distance, expressed as power of 2.
    ///
    /// Must be clamped between ZSTD_WINDOWLOG_MIN and ZSTD_WINDOWLOG_MAX.
    ///
    /// Special: value 0 means "do not change windowLog".
    ///
    /// Note: Using a window size greater than ZSTD_MAXWINDOWSIZE_DEFAULT (default: 2^27)
    ///   requires setting the maximum window size at least as large during decompression.
    WindowLog(u32),

    /// Size of the probe table, as a power of 2.
    ///
    /// Resulting table size is (1 << (hashLog+2)).
    /// Must be clamped between ZSTD_HASHLOG_MIN and ZSTD_HASHLOG_MAX.
    ///
    /// Larger tables improve compression ratio of strategies <= dFast,
    /// and improve speed of strategies > dFast.
    ///
    /// Special: value 0 means "do not change hashLog".
    HashLog(u32),

    /// Size of the full-search table, as a power of 2.
    ///
    /// Resulting table size is (1 << (chainLog+2)).
    /// Larger tables result in better and slower compression.
    /// This parameter is useless when using "fast" strategy.
    ///
    /// Special: value 0 means "do not change chainLog".
    ChainLog(u32),

    /// Number of search attempts, as a power of 2.
    ///
    /// More attempts result in better and slower compression.
    /// This parameter is useless when using "fast" and "dFast" strategies.
    ///
    /// Special: value 0 means "do not change searchLog".
    SearchLog(u32),

    /// Minimum size of searched matches (note : repCode matches can be smaller).
    ///
    /// Larger values make faster compression and decompression, but decrease ratio.
    /// Must be clamped between ZSTD_SEARCHLENGTH_MIN and ZSTD_SEARCHLENGTH_MAX.
    ///
    /// Note that currently, for all strategies < btopt, effective minimum is 4.
    ///
    /// Note that currently, for all strategies > fast, effective maximum is 6.
    ///
    /// Special: value 0 means "do not change minMatchLength".
    MinMatch(u32),

    /// Only useful for strategies >= btopt.
    ///
    /// Length of Match considered "good enough" to stop search.
    /// Larger values make compression stronger and slower.
    ///
    /// Special: value 0 means "do not change targetLength".
    TargetLength(u32),

    /// Content size will be written into frame header _whenever known_ (default:1)
    ///
    /// Content size must be known at the beginning of compression,
    /// it is provided using ZSTD_CCtx_setPledgedSrcSize()
    ContentSizeFlag(bool),

    /// A 32-bits checksum of content is written at end of frame (default:0)
    ChecksumFlag(bool),

    /// When applicable, dictionary's ID is written into frame header (default:1)
    DictIdFlag(bool),

    /// Select how many threads a compression job can spawn (default:1)
    ///
    /// More threads improve speed, but also increase memory usage.
    ///
    /// Special: value 0 means "do not change nbThreads"
    #[cfg(feature = "zstdmt")]
    ThreadCount(u32),

    /// Size of a compression job. This value is only enforced in streaming (non-blocking) mode.
    ///
    /// Each compression job is completed in parallel, so indirectly controls the nb of active threads.
    /// 0 means default, which is dynamically determined based on compression parameters.
    ///
    /// Job size must be a minimum of overlapSize, or 1 KB, whichever is largest
    ///
    /// The minimum size is automatically and transparently enforced
    #[cfg(feature = "zstdmt")]
    JobSize(u32),

    /// Size of previous input reloaded at the beginning of each job.
    ///
    /// 0 => no overlap, 6(default) => use 1/8th of windowSize, >=9 => use full windowSize
    #[cfg(feature = "zstdmt")]
    OverlapSizeLog(u32),
    // CompressionStrategy, and parameters marked as "advanced", are currently missing on purpose,
    // as they will see the most API churn.
}

/// Set one compression parameter, selected by enum ZSTD_cParameter.
///
/// @result : informational value (typically, the one being set, possibly corrected),
/// or an error code (which can be tested with ZSTD_isError()).
#[cfg(feature = "experimental")]
pub fn cctx_set_parameter(cctx: &mut CCtx, param: CParameter) -> SafeResult {
    use zstd_sys::ZSTD_cParameter;
    // TODO: Until bindgen properly generates a binding for this, we'll need to do it here.
    use zstd_sys::ZSTD_cParameter::ZSTD_c_experimentalParam2 as ZSTD_c_format;
    use zstd_sys::ZSTD_format_e;
    use CParameter::*;

    let (param, value) = match param {
        Format(FrameFormat::One) => {
            (ZSTD_c_format, ZSTD_format_e::ZSTD_f_zstd1 as c_int)
        }
        Format(FrameFormat::Magicless) => (
            ZSTD_c_format,
            ZSTD_format_e::ZSTD_f_zstd1_magicless as c_int,
        ),
        CompressionLevel(level) => {
            (ZSTD_cParameter::ZSTD_c_compressionLevel, level)
        }
        WindowLog(value) => {
            (ZSTD_cParameter::ZSTD_c_windowLog, value as c_int)
        }
        HashLog(value) => (ZSTD_cParameter::ZSTD_c_hashLog, value as c_int),
        ChainLog(value) => (ZSTD_cParameter::ZSTD_c_chainLog, value as c_int),
        SearchLog(value) => {
            (ZSTD_cParameter::ZSTD_c_searchLog, value as c_int)
        }
        MinMatch(value) => (ZSTD_cParameter::ZSTD_c_minMatch, value as c_int),
        TargetLength(value) => {
            (ZSTD_cParameter::ZSTD_c_targetLength, value as c_int)
        }
        ContentSizeFlag(flag) => (
            ZSTD_cParameter::ZSTD_c_contentSizeFlag,
            if flag { 1 } else { 0 },
        ),
        ChecksumFlag(flag) => (
            ZSTD_cParameter::ZSTD_c_checksumFlag,
            if flag { 1 } else { 0 },
        ),
        DictIdFlag(flag) => {
            (ZSTD_cParameter::ZSTD_c_dictIDFlag, if flag { 1 } else { 0 })
        }

        #[cfg(feature = "zstdmt")]
        ThreadCount(value) => {
            (ZSTD_cParameter::ZSTD_c_nbWorkers, value as c_int)
        }

        #[cfg(feature = "zstdmt")]
        JobSize(value) => (ZSTD_cParameter::ZSTD_c_jobSize, value as c_int),

        #[cfg(feature = "zstdmt")]
        OverlapSizeLog(value) => {
            (ZSTD_cParameter::ZSTD_c_overlapLog, value as c_int)
        }
    };

    let code =
        unsafe { zstd_sys::ZSTD_CCtx_setParameter(cctx.0, param, value) };
    parse_code(code)
}

/// `ZDICT_trainFromBuffer()`
///
/// Train a dictionary from an array of samples.
///
/// Samples must be stored concatenated in a single flat buffer `samplesBuffer`,
/// supplied with an array of sizes `samplesSizes`, providing the size of each sample, in order.
/// The resulting dictionary will be saved into `dictBuffer`.
///
/// Returns the size of the dictionary stored into `dictBuffer` (<= `dictBufferCapacity`)
/// or an error code, which can be tested with ZDICT_isError().
///
/// Tips : In general, a reasonable dictionary has a size of ~ 100 KB.
///        It's obviously possible to target smaller or larger ones, just by specifying different `dictBufferCapacity`.
///        In general, it's recommended to provide a few thousands samples, but this can vary a lot.
///        It's recommended that total size of all samples be about ~x100 times the target size of dictionary.
pub fn train_from_buffer(
    dict_buffer: &mut [u8],
    samples_buffer: &[u8],
    samples_sizes: &[usize],
) -> SafeResult {
    assert_eq!(samples_buffer.len(), samples_sizes.iter().sum());
    let code = unsafe {
        zstd_sys::ZDICT_trainFromBuffer(
            ptr_mut_void(dict_buffer),
            dict_buffer.len(),
            ptr_void(samples_buffer),
            samples_sizes.as_ptr(),
            samples_sizes.len() as u32,
        )
    };
    parse_code(code)
}
#[cfg(feature = "experimental")]
pub fn get_block_size(cctx: &mut CCtx) -> usize {
    unsafe { zstd_sys::ZSTD_getBlockSize(cctx.0) }
}
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
#[cfg(feature = "experimental")]
pub fn insert_block(dctx: &mut DCtx, block: &[u8]) -> usize {
    unsafe { zstd_sys::ZSTD_insertBlock(dctx.0, ptr_void(block), block.len()) }
}

/// Multi-threading methods.
#[cfg(feature = "zstdmt")]
pub mod mt {
    use super::c_ulonglong;
    use super::parse_code;
    use super::zstd_sys;
    use super::SafeResult;
    use super::{ptr_mut, ptr_mut_void, ptr_void, InBuffer, OutBuffer};

    pub struct CCtx(*mut zstd_sys::ZSTDMT_CCtx);

    pub fn create_cctx(n_threads: u32) -> CCtx {
        CCtx(unsafe { zstd_sys::ZSTDMT_createCCtx(n_threads) })
    }

    impl Drop for CCtx {
        fn drop(&mut self) {
            unsafe {
                zstd_sys::ZSTDMT_freeCCtx(self.0);
            }
        }
    }

    pub fn sizeof_cctx(cctx: &CCtx) -> usize {
        unsafe { zstd_sys::ZSTDMT_sizeof_CCtx(cctx.0) }
    }

    pub fn compress_cctx(
        mtctx: &mut CCtx,
        dst: &mut [u8],
        src: &[u8],
        compression_level: i32,
    ) -> usize {
        unsafe {
            zstd_sys::ZSTDMT_compressCCtx(
                mtctx.0,
                ptr_mut_void(dst),
                dst.len(),
                ptr_void(src),
                src.len(),
                compression_level,
            )
        }
    }

    pub fn init_cstream(mtctx: &mut CCtx, compression_level: i32) -> usize {
        unsafe { zstd_sys::ZSTDMT_initCStream(mtctx.0, compression_level) }
    }

    pub fn reset_cstream(mtctx: &mut CCtx, pledged_src_size: u64) -> usize {
        unsafe {
            zstd_sys::ZSTDMT_resetCStream(
                mtctx.0,
                pledged_src_size as c_ulonglong,
            )
        }
    }

    pub fn compress_stream(
        mtctx: &mut CCtx,
        output: &mut OutBuffer,
        input: &mut InBuffer,
    ) -> SafeResult {
        let mut output = output.wrap();
        let mut input = input.wrap();
        let code = unsafe {
            zstd_sys::ZSTDMT_compressStream(
                mtctx.0,
                ptr_mut(&mut output),
                ptr_mut(&mut input),
            )
        };
        parse_code(code)
    }

    pub fn flush_stream(
        mtctx: &mut CCtx,
        output: &mut OutBuffer,
    ) -> SafeResult {
        let mut output = output.wrap();
        let code = unsafe {
            zstd_sys::ZSTDMT_flushStream(mtctx.0, ptr_mut(&mut output))
        };
        parse_code(code)
    }

    pub fn end_stream(mtctx: &mut CCtx, output: &mut OutBuffer) -> SafeResult {
        let mut output = output.wrap();
        let code = unsafe {
            zstd_sys::ZSTDMT_endStream(mtctx.0, ptr_mut(&mut output))
        };
        parse_code(code)
    }
}
