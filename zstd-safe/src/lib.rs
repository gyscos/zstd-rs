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
pub const CLEVEL_DEFAULT: i32 = zstd_sys::ZSTD_CLEVEL_DEFAULT as i32;
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
#[cfg(feature = "experimental")]
pub const FRAMEHEADERSIZE_PREFIX: u32 = zstd_sys::ZSTD_FRAMEHEADERSIZE_PREFIX;
#[cfg(feature = "experimental")]
pub const FRAMEHEADERSIZE_MIN: u32 = zstd_sys::ZSTD_FRAMEHEADERSIZE_MIN;
#[cfg(feature = "experimental")]
pub const FRAMEHEADERSIZE_MAX: u32 = zstd_sys::ZSTD_FRAMEHEADERSIZE_MAX;

/// Wrapper result around most zstd functions.
///
/// Either a success code (usually number of bytes written), or an error code.
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

pub fn version_string() -> &'static str {
    unsafe { c_char_to_str(zstd_sys::ZSTD_versionString()) }
}

pub fn min_c_level() -> i32 {
    unsafe { zstd_sys::ZSTD_minCLevel() as i32 }
}

pub fn max_c_level() -> i32 {
    unsafe { zstd_sys::ZSTD_maxCLevel() as i32 }
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

/// `ZSTD_compress2()`
///
/// Behave the same as `ZSTD_compressCCtx()`, but compression parameters are set using the advanced API.
/// `ZSTD_compress2()` always starts a new frame.
///
/// Should cctx hold data from a previously unfinished frame, everything about it is forgotten.
/// - Compression parameters are pushed into CCtx before starting compression, using `ZSTD_CCtx_set*()`
/// - The function is always blocking, returns when compression is completed.
///
/// Hint : compression runs faster if `dstCapacity` >=  `ZSTD_compressBound(srcSize)`.
///
/// Return : compressed size written into `dst` (<= `dstCapacity),
///          or an error code if it fails (which can be tested using `ZSTD_isError()`).
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

pub struct DCtx<'a>(*mut zstd_sys::ZSTD_DCtx, PhantomData<&'a ()>);

impl Default for DCtx<'_> {
    fn default() -> Self {
        create_dctx()
    }
}

pub fn create_dctx<'a>() -> DCtx<'a> {
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

pub type CStream<'a> = CCtx<'a>;

// CStream can't be shared across threads, so it does not implement Sync.

pub fn create_cstream<'a>() -> CStream<'a> {
    CCtx(unsafe { zstd_sys::ZSTD_createCStream() }, PhantomData)
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

/// `ZSTD_compressStream2()`
///
/// Behaves about the same as `ZSTD_compressStream`, with additional control on end directive.
/// - Compression parameters are pushed into CCtx before starting compression, using ZSTD_CCtx_set*()
/// - Compression parameters cannot be changed once compression is started (save a list of exceptions in multi-threading mode)
/// - output->pos must be <= dstCapacity, input->pos must be <= srcSize
/// - output->pos and input->pos will be updated. They are guaranteed to remain below their respective limit.
/// - When nbWorkers==0 (default), function is blocking : it completes its job before returning to caller.
/// - When nbWorkers>=1, function is non-blocking : it just acquires a copy of input, and distributes jobs to internal worker threads, flush whatever is available,
///                                                 and then immediately returns, just indicating that there is some data remaining to be flushed.
///                                                 The function nonetheless guarantees forward progress : it will return only after it reads or write at least 1+ byte.
/// - Exception : if the first call requests a ZSTD_e_end directive and provides enough dstCapacity, the function delegates to ZSTD_compress2() which is always blocking.
/// - @return provides a minimum amount of data remaining to be flushed from internal buffers
///           or an error code, which can be tested using ZSTD_isError().
///           if @return != 0, flush is not fully completed, there is still some data left within internal buffers.
///           This is useful for ZSTD_e_flush, since in this case more flushes are necessary to empty all buffers.
///           For ZSTD_e_end, @return == 0 when internal buffers are fully flushed and frame is completed.
/// - after a ZSTD_e_end directive, if internal buffer is not fully flushed (@return != 0),
///           only ZSTD_e_end or ZSTD_e_flush operations are allowed.
///           Before starting a new compression job, or changing compression parameters,
///           it is required to fully flush internal buffers.
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

pub type DStream<'a> = DCtx<'a>;

pub fn create_dstream<'a>() -> DStream<'a> {
    DCtx(unsafe { zstd_sys::ZSTD_createDStream() }, PhantomData)
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
pub fn sizeof_cctx(cctx: &CCtx) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_CCtx(cctx.0) }
}

/// `ZSTD_sizeof_DCtx()`
///
/// Gives the amount of memory used by a given ZSTD_DCtx
pub fn sizeof_dctx(dctx: &DCtx) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_DCtx(dctx.0) }
}

pub fn sizeof_cstream(zcs: &CStream) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_CStream(zcs.0) }
}

pub fn sizeof_dstream(zds: &DStream) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_DStream(zds.0) }
}

/// `ZSTD_sizeof_CDict()`
///
/// Gives the amount of memory used by a given ZSTD_sizeof_CDict
pub fn sizeof_cdict(cdict: &CDict) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_CDict(cdict.0) }
}
/// `ZSTD_sizeof_DDict()`
///
/// Gives the amount of memory used by a given ZSTD_DDict
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
pub fn create_cdict_by_reference<'a>(
    dict_buffer: &[u8],
    compression_level: i32,
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

/// `ZSTD_CCtx_loadDictionary()`
///
/// Create an internal CDict from `dict` buffer.
///
/// Decompression will have to use same dictionary.
///
/// result : 0, or an error code (which can be tested with ZSTD_isError()).
///
/// Special: Loading a NULL (or 0-size) dictionary invalidates previous dictionary,
///          meaning "return to no-dictionary mode".
///
/// Note 1 : Dictionary is sticky, it will be used for all future compressed frames.
///          To return to "no-dictionary" situation, load a NULL dictionary (or reset parameters).
///
/// Note 2 : Loading a dictionary involves building tables.
///          It's also a CPU consuming operation, with non-negligible impact on latency.
///          Tables are dependent on compression parameters, and for this reason,
///          compression parameters can no longer be changed after loading a dictionary.
///
/// Note 3 :`dict` content will be copied internally.
///          Use experimental ZSTD_CCtx_loadDictionary_byReference() to reference content instead.
///          In such a case, dictionary buffer must outlive its users.
///
/// Note 4 : Use ZSTD_CCtx_loadDictionary_advanced()
///          to precisely select how dictionary content must be interpreted. */
pub fn cctx_load_dictionary(cctx: &mut CCtx, dict: &[u8]) -> SafeResult {
    parse_code(unsafe {
        zstd_sys::ZSTD_CCtx_loadDictionary(cctx.0, ptr_void(dict), dict.len())
    })
}

/// `ZSTD_CCtx_refCDict()`
///
/// Reference a prepared dictionary, to be used for all next compressed frames.
///
/// Note that compression parameters are enforced from within CDict,
/// and supersede any compression parameter previously set within CCtx.
///
/// The parameters ignored are labled as "superseded-by-cdict" in the ZSTD_cParameter enum docs.
///
/// The ignored parameters will be used again if the CCtx is returned to no-dictionary mode.
///
/// The dictionary will remain valid for future compressed frames using same CCtx.
///
/// result : 0, or an error code (which can be tested with ZSTD_isError()).
///
/// Special : Referencing a NULL CDict means "return to no-dictionary mode".
/// Note 1 : Currently, only one dictionary can be managed.
///          Referencing a new dictionary effectively "discards" any previous one.
/// Note 2 : CDict is just referenced, its lifetime must outlive its usage within CCtx. */
pub fn cctx_ref_cdict<'a, 'b>(
    cctx: &mut CCtx<'a>,
    cdict: &'b CDict<'a>,
) -> SafeResult
where
    'b: 'a,
{
    parse_code(unsafe { zstd_sys::ZSTD_CCtx_refCDict(cctx.0, cdict.0) })
}

/// `ZSTD_CCtx_refPrefix()`
///
/// Reference a prefix (single-usage dictionary) for next compressed frame.
///
/// A prefix is **only used once**. Tables are discarded at end of frame (ZSTD_e_end).
/// Decompression will need same prefix to properly regenerate data.
///
/// Compressing with a prefix is similar in outcome as performing a diff and compressing it,
/// but performs much faster, especially during decompression (compression speed is tunable with compression level).
///
/// result : 0, or an error code (which can be tested with ZSTD_isError()).
///
/// Special: Adding any prefix (including NULL) invalidates any previous prefix or dictionary
///
/// Note 1 : Prefix buffer is referenced. It **must** outlive compression.
///          Its content must remain unmodified during compression.
///
/// Note 2 : If the intention is to diff some large src data blob with some prior version of itself,
///          ensure that the window size is large enough to contain the entire source.
///          See ZSTD_c_windowLog.
///
/// Note 3 : Referencing a prefix involves building tables, which are dependent on compression parameters.
///          It's a CPU consuming operation, with non-negligible impact on latency.
///          If there is a need to use the same prefix multiple times, consider loadDictionary instead.
///
/// Note 4 : By default, the prefix is interpreted as raw content (ZSTD_dm_rawContent).
///          Use experimental ZSTD_CCtx_refPrefix_advanced() to alter dictionary interpretation. */
pub fn cctx_ref_prefix<'a>(
    cctx: &mut CCtx<'a>,
    prefix: &'a [u8],
) -> SafeResult {
    parse_code(unsafe {
        zstd_sys::ZSTD_CCtx_refPrefix(cctx.0, ptr_void(prefix), prefix.len())
    })
}

/// `ZSTD_DCtx_loadDictionary()`
///
/// Create an internal DDict from dict buffer,
/// to be used to decompress next frames.
///
/// The dictionary remains valid for all future frames, until explicitly invalidated.
///
/// result : 0, or an error code (which can be tested with ZSTD_isError()).
///
/// Special : Adding a NULL (or 0-size) dictionary invalidates any previous dictionary,
///           meaning "return to no-dictionary mode".
/// Note 1 : Loading a dictionary involves building tables,
///          which has a non-negligible impact on CPU usage and latency.
///          It's recommended to "load once, use many times", to amortize the cost
/// Note 2 :`dict` content will be copied internally, so `dict` can be released after loading.
///          Use ZSTD_DCtx_loadDictionary_byReference() to reference dictionary content instead.
/// Note 3 : Use ZSTD_DCtx_loadDictionary_advanced() to take control of
///          how dictionary content is loaded and interpreted.
pub fn dctx_load_dictionary(dctx: &mut DCtx<'_>, dict: &[u8]) -> SafeResult {
    parse_code(unsafe {
        zstd_sys::ZSTD_DCtx_loadDictionary(dctx.0, ptr_void(dict), dict.len())
    })
}

/// `ZSTD_DCtx_refDDict()`
///
/// Reference a prepared dictionary, to be used to decompress next frames.
///
/// The dictionary remains active for decompression of future frames using same DCtx.
///
/// result : 0, or an error code (which can be tested with ZSTD_isError()).
///
/// Note 1 : Currently, only one dictionary can be managed.
///          Referencing a new dictionary effectively "discards" any previous one.
/// Special: referencing a NULL DDict means "return to no-dictionary mode".
///
/// Note 2 : DDict is just referenced, its lifetime must outlive its usage from DCtx.
pub fn dctx_ref_ddict<'a, 'b>(
    dctx: &mut DCtx<'a>,
    ddict: &'b DDict<'a>,
) -> SafeResult
where
    'b: 'a,
{
    parse_code(unsafe { zstd_sys::ZSTD_DCtx_refDDict(dctx.0, ddict.0) })
}

/// `ZSTD_DCtx_refPrefix()`
///
/// Reference a prefix (single-usage dictionary) to decompress next frame.
///
/// This is the reverse operation of ZSTD_CCtx_refPrefix(),
/// and must use the same prefix as the one used during compression.
///
/// Prefix is **only used once**. Reference is discarded at end of frame.
///
/// End of frame is reached when ZSTD_decompressStream() returns 0.
///
/// result : 0, or an error code (which can be tested with ZSTD_isError()).
///
/// Note 1 : Adding any prefix (including NULL) invalidates any previously set prefix or dictionary
///
/// Note 2 : Prefix buffer is referenced. It **must** outlive decompression.
///          Prefix buffer must remain unmodified up to the end of frame,
///          reached when ZSTD_decompressStream() returns 0.
///
/// Note 3 : By default, the prefix is treated as raw content (ZSTD_dm_rawContent).
///          Use ZSTD_CCtx_refPrefix_advanced() to alter dictMode (Experimental section)
///
/// Note 4 : Referencing a raw content prefix has almost no cpu nor memory cost.
///          A full dictionary is more costly, as it requires building tables.
pub fn dctx_ref_prefix<'a>(
    dctx: &mut DCtx<'a>,
    prefix: &'a [u8],
) -> SafeResult {
    parse_code(unsafe {
        zstd_sys::ZSTD_DCtx_refPrefix(dctx.0, ptr_void(prefix), prefix.len())
    })
}

/// `ZSTD_CCtx_reset()`
///
/// There are 2 different things that can be reset, independently or jointly :
/// - The session : will stop compressing current frame, and make CCtx ready to start a new one.
///                 Useful after an error, or to interrupt any ongoing compression.
///                 Any internal data not yet flushed is cancelled.
///                 Compression parameters and dictionary remain unchanged.
///                 They will be used to compress next frame.
///                 Resetting session never fails.
/// - The parameters : changes all parameters back to "default".
///                 This removes any reference to any dictionary too.
///                 Parameters can only be changed between 2 sessions (i.e. no compression is currently ongoing)
///                 otherwise the reset fails, and function returns an error value (which can be tested using ZSTD_isError())
/// - Both : similar to resetting the session, followed by resetting parameters.
///
pub fn cctx_reset(cctx: &mut CCtx, reset: ResetDirective) -> SafeResult {
    parse_code(unsafe { zstd_sys::ZSTD_CCtx_reset(cctx.0, reset) })
}

/// `ZSTD_DCtx_reset()`
///
/// Return a DCtx to clean state.
///
/// Session and parameters can be reset jointly or separately.
///
/// Parameters can only be reset when no active frame is being decompressed.
///
/// return : 0, or an error code, which can be tested with ZSTD_isError()
pub fn dctx_reset(dctx: &mut DCtx, reset: ResetDirective) -> SafeResult {
    parse_code(unsafe { zstd_sys::ZSTD_DCtx_reset(dctx.0, reset) })
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
    #[cfg(feature = "experimental")]
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

    /// Compression strategy. Affects compression ratio and speed.
    Strategy(Strategy),

    /// Enables long distance matching to improve compression ratio for large inputs.
    ///
    /// Increases memory usage and window size.
    EnableLongDistanceMatching(bool),

    /// Size of the table for long distance matching, as a power of 2.
    ///
    /// Larger values increase memory usage and compression ratio, but decrease compression speed.
    /// Must be clamped between ZSTD_HASHLOG_MIN and ZSTD_HASHLOG_MAX
    ///
    /// Default: `windowlog - 7`.
    ///
    /// Special: value 0 means "automatically determine hashlog".
    LdmHashLog(u32),

    /// Minimum match size for long distance matcher.
    ///
    /// Larger/too small values usually decrease compression ratio.
    ///
    /// Must be clamped between `ZSTD_LDM_MINMATCH_MIN` and `ZSTD_LDM_MINMATCH_MAX`.
    ///
    /// Special: value 0 means "use default value" (default: 64).
    LdmMinMatch(u32),

    /// Log size of each bucket in the LDM hash table for collision resolution.
    ///
    /// Larger values improve collision resolution but decrease compression speed.
    /// The maximum value is `ZSTD_LDM_BUCKETSIZELOG_MAX`.
    ///
    /// Special: value 0 means "use default value" (default: 3).
    LdmBucketSizeLog(u32),

    /// Frequency of inserting/looking up entries into the LDM hash table.
    ///
    /// Must be clamped between 0 and `(ZSTD_WINDOWLOG_MAX - ZSTD_HASHLOG_MIN)`.
    /// Default is `MAX(0, (windowLog - ldmHashLog))`, optimizing hash table usage.
    /// Larger values improve compression speed.
    ///
    /// Deviating far from default value will likely result in a compression ratio decrease.
    ///
    /// Special: value 0 means "automatically determine hashRateLog".
    LdmHashRateLog(u32),

    /// Content size will be written into frame header _whenever known_ (default:1)
    ///
    /// Content size must be known at the beginning of compression,
    /// it is provided using ZSTD_CCtx_setPledgedSrcSize()
    ContentSizeFlag(bool),

    /// A 32-bits checksum of content is written at end of frame (default:0)
    ChecksumFlag(bool),

    /// When applicable, dictionary's ID is written into frame header (default:1)
    DictIdFlag(bool),

    /// Select how many threads will be spawned to compress in parallel.
    ///
    /// When nbWorkers >= 1, triggers asynchronous mode when used with ZSTD_compressStream*() :
    /// `ZSTD_compressStream*()` consumes input and flush output if possible, but immediately gives back control to caller,
    /// while compression work is performed in parallel, within worker threads.
    ///
    /// (note : a strong exception to this rule is when first invocation of `ZSTD_compressStream2()` sets `ZSTD_e_end` :
    ///  in which case, `ZSTD_compressStream2()` delegates to `ZSTD_compress2()`, which is always a blocking call).
    ///
    /// More workers improve speed, but also increase memory usage.
    ///
    /// Default value is `0`, aka "single-threaded mode" : no worker is spawned, compression is performed inside Caller's thread, all invocations are blocking.
    NbWorkers(u32),

    /// Size of a compression job. This value is enforced only when `nbWorkers >= 1`.
    ///
    /// Each compression job is completed in parallel, so this value can indirectly impact the nb of active threads.
    ///
    /// 0 means default, which is dynamically determined based on compression parameters.
    ///
    /// Job size must be a minimum of overlap size, or 1 MB, whichever is largest.
    ///
    /// The minimum size is automatically and transparently enforced
    JobSize(u32),

    /// Control the overlap size, as a fraction of window size.
    ///
    /// The overlap size is an amount of data reloaded from previous job at the beginning of a new job.
    ///
    /// It helps preserve compression ratio, while each job is compressed in parallel.
    ///
    /// This value is enforced only when nbWorkers >= 1.
    ///
    /// Larger values increase compression ratio, but decrease speed.
    ///
    /// Possible values range from 0 to 9 :
    /// - 0 means "default" : value will be determined by the library, depending on strategy
    /// - 1 means "no overlap"
    /// - 9 means "full overlap", using a full window size.
    ///
    /// Each intermediate rank increases/decreases load size by a factor 2 :
    /// 9: full window;  8: w/2;  7: w/4;  6: w/8;  5:w/16;  4: w/32;  3:w/64;  2:w/128;  1:no overlap;  0:default
    ///
    /// default value varies between 6 and 9, depending on strategy.
    OverlapSizeLog(u32),
}

pub enum DParameter {
    /// Select a size limit (in power of 2) beyond which
    /// the streaming API will refuse to allocate memory buffer
    /// in order to protect the host from unreasonable memory requirements.
    ///
    /// This parameter is only useful in streaming mode, since no internal buffer is allocated in single-pass mode.
    ///
    /// By default, a decompression context accepts window sizes <= `(1 << ZSTD_WINDOWLOG_LIMIT_DEFAULT)`.
    ///
    /// Special: value 0 means "use default maximum windowLog". */
    WindowLogMax(u32),

    /// See `FrameFormat`.
    #[cfg(feature = "experimental")]
    Format(FrameFormat),
}

/// `ZSTD_DCtx_setParameter()`
///
/// Set one compression parameter, selected by enum ZSTD_dParameter.
///
/// All parameters have valid bounds. Bounds can be queried using ZSTD_dParam_getBounds().
///
/// Providing a value beyond bound will either clamp it, or trigger an error (depending on parameter).
///
/// Setting a parameter is only possible during frame initialization (before starting decompression).
///
/// return : 0, or an error code (which can be tested using ZSTD_isError()).
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

/// Set one compression parameter, selected by enum ZSTD_cParameter.
///
/// @result : informational value (typically, the one being set, possibly corrected),
/// or an error code (which can be tested with ZSTD_isError()).
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

/// `ZSTD_CCtx_setPledgedSrcSize()`
///
/// Total input data size to be compressed as a single frame.
///
/// Value will be written in frame header, unless if explicitly forbidden using ZSTD_c_contentSizeFlag.
///
/// This value will also be controlled at end of frame, and trigger an error if not respected.
///
/// result : 0, or an error code (which can be tested with ZSTD_isError()).
///
/// Note 1 : pledgedSrcSize==0 actually means zero, aka an empty frame.
///          In order to mean "unknown content size", pass constant ZSTD_CONTENTSIZE_UNKNOWN.
///          ZSTD_CONTENTSIZE_UNKNOWN is default value for any new frame.
///
/// Note 2 : pledgedSrcSize is only valid once, for the next frame.
///          It's discarded at the end of the frame, and replaced by ZSTD_CONTENTSIZE_UNKNOWN.
///
/// Note 3 : Whenever all input data is provided and consumed in a single round,
///          for example with ZSTD_compress2(),
///          or invoking immediately ZSTD_compressStream2(,,,ZSTD_e_end),
///          this value is automatically overridden by srcSize instead.
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
