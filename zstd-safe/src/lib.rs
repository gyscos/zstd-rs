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

extern crate zstd_sys;
extern crate libc;



use core::marker::PhantomData;
use core::ops::Deref;
use core::ops::DerefMut;
use core::slice;
use core::str;

fn ptr_void(src: &[u8]) -> *const libc::c_void {
    src.as_ptr() as *const libc::c_void
}

fn ptr_mut_void(dst: &mut [u8]) -> *mut libc::c_void {
    dst.as_mut_ptr() as *mut libc::c_void
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
pub fn compress(dst: &mut [u8], src: &[u8], compression_level: i32) -> usize {
    unsafe {
        zstd_sys::ZSTD_compress(ptr_mut_void(dst),
                                dst.len(),
                                ptr_void(src),
                                src.len(),
                                compression_level)
    }
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
pub fn decompress(dst: &mut [u8], src: &[u8]) -> usize {
    unsafe {
        zstd_sys::ZSTD_decompress(ptr_mut_void(dst),
                                  dst.len(),
                                  ptr_void(src),
                                  src.len())
    }

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

pub fn is_error(code: usize) -> u32 {
    unsafe { zstd_sys::ZSTD_isError(code) as u32 }
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

pub fn get_error_name(code: usize) -> &'static str {
    unsafe {
        // We are getting a *const char from zstd
        let name = zstd_sys::ZSTD_getErrorName(code);
        // To be safe, we need to compute right now its length
        let len = libc::strlen(name);
        // Cast it to a slice
        let slice = slice::from_raw_parts(name as *mut u8, len);
        // And hope it's still text.
        str::from_utf8(slice).expect("bad error message from zstd")
    }
}

/// `ZSTD_compressCCtx()`
///
/// Same as `ZSTD_compress()`, requires an allocated `ZSTD_CCtx` (see `ZSTD_createCCtx()`).
pub fn compress_cctx(ctx: &mut CCtx, dst: &mut [u8], src: &[u8],
                     compression_level: i32)
                     -> usize {
    unsafe {
        zstd_sys::ZSTD_compressCCtx(ctx.0,
                                    ptr_mut_void(dst),
                                    dst.len(),
                                    ptr_void(src),
                                    src.len(),
                                    compression_level)
    }
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
pub fn decompress_dctx(ctx: &mut DCtx, dst: &mut [u8], src: &[u8]) -> usize {
    unsafe {
        zstd_sys::ZSTD_decompressDCtx(ctx.0,
                                      ptr_mut_void(dst),
                                      dst.len(),
                                      ptr_void(src),
                                      src.len())
    }
}

/// `ZSTD_compress_usingDict()`
///
/// Compression using a predefined Dictionary (see dictBuilder/zdict.h).
///
/// Note : This function loads the dictionary, resulting in significant startup delay.
///
/// Note : When `dict == NULL || dictSize < 8` no dictionary is used.
pub fn compress_using_dict(ctx: &mut CCtx, dst: &mut [u8], src: &[u8],
                           dict: &[u8], compression_level: i32)
                           -> usize {
    unsafe {
        zstd_sys::ZSTD_compress_usingDict(ctx.0,
                                          ptr_mut_void(dst),
                                          dst.len(),
                                          ptr_void(src),
                                          src.len(),
                                          ptr_void(dict),
                                          dict.len(),
                                          compression_level)
    }
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
pub fn decompress_using_dict(dctx: &mut DCtx, dst: &mut [u8], src: &[u8],
                             dict: &[u8])
                             -> usize {

    unsafe {
        zstd_sys::ZSTD_decompress_usingDict(dctx.0,
                                            ptr_mut_void(dst),
                                            dst.len(),
                                            ptr_void(src),
                                            src.len(),
                                            ptr_void(dict),
                                            dict.len())
    }
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
pub fn create_cdict(dict_buffer: &[u8], compression_level: i32)
                    -> CDict<'static> {
    CDict(unsafe {
              zstd_sys::ZSTD_createCDict(ptr_void(dict_buffer),
                                         dict_buffer.len(),
                                         compression_level)
          },
          PhantomData)
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
pub fn compress_using_cdict(cctx: &mut CCtx, dst: &mut [u8], src: &[u8],
                            cdict: &CDict)
                            -> usize {
    unsafe {
        zstd_sys::ZSTD_compress_usingCDict(cctx.0,
                                           ptr_mut_void(dst),
                                           dst.len(),
                                           ptr_void(src),
                                           src.len(),
                                           cdict.0)
    }

}

pub struct DDict<'a>(*mut zstd_sys::ZSTD_DDict, PhantomData<&'a ()>);

/// `ZSTD_createDDict()`
///
/// Create a digested dictionary, ready to start decompression operation without startup delay.
///
/// dictBuffer can be released after DDict creation, as its content is copied inside DDict
pub fn create_ddict(dict_buffer: &[u8]) -> DDict<'static> {
    DDict(unsafe {
              zstd_sys::ZSTD_createDDict(ptr_void(dict_buffer),
                                         dict_buffer.len())
          },
          PhantomData)
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
pub fn decompress_using_ddict(dctx: &mut DCtx, dst: &mut [u8], src: &[u8],
                              ddict: &DDict)
                              -> usize {
    unsafe {
        zstd_sys::ZSTD_decompress_usingDDict(dctx.0,
                                             ptr_mut_void(dst),
                                             dst.len(),
                                             ptr_void(src),
                                             src.len(),
                                             ddict.0)
    }
}

pub struct CStream(*mut zstd_sys::ZSTD_CStream);

impl Default for CStream {
    fn default() -> Self {
        create_cstream()
    }
}

pub fn create_cstream() -> CStream {
    CStream(unsafe { zstd_sys::ZSTD_createCStream() })
}

impl Drop for CStream {
    fn drop(&mut self) {
        unsafe {
            zstd_sys::ZSTD_freeCStream(self.0);
        }
    }
}

unsafe impl Send for CStream {}
// CStream can't be shared across threads, so it does not implement Sync.

pub fn init_cstream(zcs: &mut CStream, compression_level: i32) -> usize {
    unsafe { zstd_sys::ZSTD_initCStream(zcs.0, compression_level) }
}


pub struct InBuffer<'a> {
    pub src: &'a [u8],
    pub pos: usize,
}

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

pub fn compress_stream(zcs: &mut CStream, output: &mut OutBuffer,
                       input: &mut InBuffer)
                       -> usize {
    let mut output = output.wrap();
    let mut input = input.wrap();
    unsafe {
        zstd_sys::ZSTD_compressStream(zcs.0,
                                      ptr_mut(&mut output),
                                      ptr_mut(&mut input))
    }
}

pub fn flush_stream(zcs: &mut CStream, output: &mut OutBuffer) -> usize {
    let mut output = output.wrap();
    unsafe { zstd_sys::ZSTD_flushStream(zcs.0, ptr_mut(&mut output)) }
}

pub fn end_stream(zcs: &mut CStream, output: &mut OutBuffer) -> usize {
    let mut output = output.wrap();
    unsafe { zstd_sys::ZSTD_endStream(zcs.0, ptr_mut(&mut output)) }
}

pub fn cstream_in_size() -> usize {
    unsafe { zstd_sys::ZSTD_CStreamInSize() }
}

pub fn cstream_out_size() -> usize {
    unsafe { zstd_sys::ZSTD_CStreamOutSize() }
}

pub struct DStream(*mut zstd_sys::ZSTD_DStream);

impl Default for DStream {
    fn default() -> Self {
        create_dstream()
    }
}

pub fn create_dstream() -> DStream {
    DStream(unsafe { zstd_sys::ZSTD_createDStream() })
}

impl Drop for DStream {
    fn drop(&mut self) {
        unsafe {
            zstd_sys::ZSTD_freeDStream(self.0);
        }
    }
}

unsafe impl Send for DStream {}
// DStream can't be shared across threads, so it does not implement Sync.

pub fn init_dstream(zds: &mut DStream) -> usize {
    unsafe { zstd_sys::ZSTD_initDStream(zds.0) }
}

pub fn decompress_stream(zds: &mut DStream, output: &mut OutBuffer,
                         input: &mut InBuffer)
                         -> usize {
    let mut output = output.wrap();
    let mut input = input.wrap();
    unsafe {
        zstd_sys::ZSTD_decompressStream(zds.0,
                                        ptr_mut(&mut output),
                                        ptr_mut(&mut input))
    }
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
pub fn find_frame_compressed_size(src: &[u8]) -> usize {
    unsafe { zstd_sys::ZSTD_findFrameCompressedSize(ptr_void(src), src.len()) }

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
pub fn create_cdict_by_reference<'a>(dict_buffer: &'a [u8],
                                     compression_level: i32)
                                     -> CDict<'a> {
    CDict(unsafe {
              zstd_sys::ZSTD_createCDict_byReference(ptr_void(dict_buffer),
                                                     dict_buffer.len(),
                                                     compression_level)
          },
          PhantomData)
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
pub fn create_ddict_by_reference<'a>(dict_buffer: &'a [u8]) -> DDict<'a> {
    DDict(unsafe {
              zstd_sys::ZSTD_createDDict_byReference(ptr_void(dict_buffer),
                                                     dict_buffer.len())
          },
          PhantomData)
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
pub fn init_cstream_src_size(zcs: &mut CStream, compression_level: i32,
                             pledged_src_size: u64)
                             -> usize {
    unsafe {
        zstd_sys::ZSTD_initCStream_srcSize(zcs.0,
                                           compression_level as libc::c_int,
                                           pledged_src_size as
                                           libc::c_ulonglong)
    }
}
pub fn init_cstream_using_dict(zcs: &mut CStream, dict: &[u8],
                               compression_level: i32)
                               -> usize {
    unsafe {
        zstd_sys::ZSTD_initCStream_usingDict(zcs.0,
                                             ptr_void(dict),
                                             dict.len(),
                                             compression_level)
    }
}
pub fn init_cstream_using_csict(zcs: &mut CStream, cdict: &CDict) -> usize {
    unsafe { zstd_sys::ZSTD_initCStream_usingCDict(zcs.0, cdict.0) }
}

/// `ZSTD_resetCStream()`
///
/// start a new compression job, using same parameters from previous job.
///
/// This is typically useful to skip dictionary loading stage, since it will re-use it in-place..
///
/// Note that zcs must be init at least once before using ZSTD_resetCStream().
///
/// pledgedSrcSize==0 means "srcSize unknown".
///
/// If pledgedSrcSize > 0, its value must be correct, as it will be written in header, and controlled at the end.
///
/// Returns 0, or an error code (which can be tested using ZSTD_isError()) */
pub fn reset_cstream(zcs: &mut CStream, pledged_src_size: u64) -> usize {
    unsafe {
        zstd_sys::ZSTD_resetCStream(zcs.0,
                                    pledged_src_size as libc::c_ulonglong)
    }
}
pub fn init_dstream_using_dict(zds: &mut DStream, dict: &[u8]) -> usize {
    unsafe {
        zstd_sys::ZSTD_initDStream_usingDict(zds.0, ptr_void(dict), dict.len())
    }
}
pub fn init_dstream_using_ddict(zds: &mut DStream, ddict: &DDict) -> usize {
    unsafe { zstd_sys::ZSTD_initDStream_usingDDict(zds.0, ddict.0) }
}
pub fn reset_dstream(zds: &mut DStream) -> usize {
    unsafe { zstd_sys::ZSTD_resetDStream(zds.0) }
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
pub fn train_from_buffer(dict_buffer: &mut [u8], samples_buffer: &[u8],
                         samples_sizes: &[usize])
                         -> usize {
    assert_eq!(samples_buffer.len(), samples_sizes.iter().sum());
    unsafe {
        zstd_sys::ZDICT_trainFromBuffer(ptr_mut_void(dict_buffer),
                                        dict_buffer.len(),
                                        ptr_void(samples_buffer),
                                        samples_sizes.as_ptr(),
                                        samples_sizes.len() as u32)
    }
}
pub fn get_block_size(cctx: &mut CCtx) -> usize {
    unsafe { zstd_sys::ZSTD_getBlockSize(cctx.0) }
}
pub fn compress_block(cctx: &mut CCtx, dst: &mut [u8], src: &[u8]) -> usize {
    unsafe {
        zstd_sys::ZSTD_compressBlock(cctx.0,
                                     ptr_mut_void(dst),
                                     dst.len(),
                                     ptr_void(src),
                                     src.len())
    }
}
pub fn decompress_block(dctx: &mut DCtx, dst: &mut [u8], src: &[u8]) -> usize {
    unsafe {
        zstd_sys::ZSTD_decompressBlock(dctx.0,
                                       ptr_mut_void(dst),
                                       dst.len(),
                                       ptr_void(src),
                                       src.len())
    }
}
pub fn insert_block(dctx: &mut DCtx, block: &[u8]) -> usize {
    unsafe { zstd_sys::ZSTD_insertBlock(dctx.0, ptr_void(block), block.len()) }
}


/// Multi-threading methods.
#[cfg(feature = "zstdmt")]
pub mod mt {
    use super::{ptr_void, ptr_mut, ptr_mut_void, InBuffer, OutBuffer};
    use super::libc;
    use super::zstd_sys;

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

    pub fn compress_cctx(mtctx: &mut CCtx, dst: &mut [u8], src: &[u8],
                         compression_level: i32)
                         -> usize {
        unsafe {
            zstd_sys::ZSTDMT_compressCCtx(mtctx.0,
                                          ptr_mut_void(dst),
                                          dst.len(),
                                          ptr_void(src),
                                          src.len(),
                                          compression_level)
        }
    }

    pub fn init_cstream(mtctx: &mut CCtx, compression_level: i32) -> usize {
        unsafe { zstd_sys::ZSTDMT_initCStream(mtctx.0, compression_level) }
    }

    pub fn reset_cstream(mtctx: &mut CCtx, pledged_src_size: u64) -> usize {
        unsafe {
            zstd_sys::ZSTDMT_resetCStream(mtctx.0,
                                          pledged_src_size as
                                          libc::c_ulonglong)
        }
    }

    pub fn compress_stream(mtctx: &mut CCtx, output: &mut OutBuffer,
                           input: &mut InBuffer)
                           -> usize {
        let mut output = output.wrap();
        let mut input = input.wrap();
        unsafe {
            zstd_sys::ZSTDMT_compressStream(mtctx.0,
                                            ptr_mut(&mut output),
                                            ptr_mut(&mut input))
        }
    }

    pub fn flush_stream(mtctx: &mut CCtx, output: &mut OutBuffer) -> usize {
        let mut output = output.wrap();
        unsafe { zstd_sys::ZSTDMT_flushStream(mtctx.0, ptr_mut(&mut output)) }
    }

    pub fn end_stream(mtctx: &mut CCtx, output: &mut OutBuffer) -> usize {
        let mut output = output.wrap();
        unsafe { zstd_sys::ZSTDMT_endStream(mtctx.0, ptr_mut(&mut output)) }
    }
}
