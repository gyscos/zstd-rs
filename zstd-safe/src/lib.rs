//! Minimal safe wrapper around zstd-sys.
//!
//! This crates provides a minimal translation of the zstd-sys methods.
//!
//! Check zstd's own documentation for information on specific functions.

extern crate zstd_sys;
extern crate libc;



use std::marker::PhantomData;
use std::ops::Deref;
use std::ops::DerefMut;

fn ptr_void(src: &[u8]) -> *const libc::c_void {
    src.as_ptr() as *const libc::c_void
}

fn ptr_mut_void(dst: &mut [u8]) -> *mut libc::c_void {
    dst.as_mut_ptr() as *mut libc::c_void
}

pub fn version_number() -> u32 {
    unsafe { zstd_sys::ZSTD_versionNumber() as u32 }
}

pub fn compress(dst: &mut [u8], src: &[u8], compression_level: i32) -> usize {
    unsafe {
        zstd_sys::ZSTD_compress(ptr_mut_void(dst),
                                dst.len(),
                                ptr_void(src),
                                src.len(),
                                compression_level)
    }
}

pub fn decompress(dst: &mut [u8], src: &[u8]) -> usize {
    unsafe {
        zstd_sys::ZSTD_decompress(ptr_mut_void(dst),
                                  dst.len(),
                                  ptr_void(src),
                                  src.len())
    }

}

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

pub struct CCtx(*mut zstd_sys::ZSTD_CCtx_s);

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

pub fn get_error_name(code: usize) -> &'static std::ffi::CStr {
    unsafe { std::ffi::CStr::from_ptr(zstd_sys::ZSTD_getErrorName(code)) }
}

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

pub fn decompress_dctx(ctx: &mut DCtx, dst: &mut [u8], src: &[u8]) -> usize {
    unsafe {
        zstd_sys::ZSTD_decompressDCtx(ctx.0,
                                      ptr_mut_void(dst),
                                      dst.len(),
                                      ptr_void(src),
                                      src.len())
    }
}

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

pub fn ceate_ddict(dict_buffer: &[u8]) -> DDict<'static> {
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


pub fn find_frame_compressed_size(src: &[u8]) -> usize {
    unsafe { zstd_sys::ZSTD_findFrameCompressedSize(ptr_void(src), src.len()) }

}
pub fn get_frame_content_size(src: &[u8]) -> u64 {
    unsafe { zstd_sys::ZSTD_getFrameContentSize(ptr_void(src), src.len()) }
}
pub fn find_decompressed_size(src: &[u8]) -> u64 {
    unsafe { zstd_sys::ZSTD_findDecompressedSize(ptr_void(src), src.len()) }
}

pub fn sizeof_cctx(cctx: &CCtx) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_CCtx(cctx.0) }
}

pub fn sizeof_dctx(dctx: &DCtx) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_DCtx(dctx.0) }
}

pub fn sizeof_cstream(zcs: &CStream) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_CStream(zcs.0) }
}
pub fn sizeof_dstream(zds: &DStream) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_DStream(zds.0) }
}
pub fn sizeof_cdict(cdict: &CDict) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_CDict(cdict.0) }
}
pub fn sizeof_ddict(ddict: &DDict) -> usize {
    unsafe { zstd_sys::ZSTD_sizeof_DDict(ddict.0) }
}
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

pub fn is_frame(buffer: &[u8]) -> u32 {
    unsafe { zstd_sys::ZSTD_isFrame(ptr_void(buffer), buffer.len()) as u32 }
}

pub fn create_ddict_by_reference<'a>(dict_buffer: &'a [u8]) -> DDict<'a> {
    DDict(unsafe {
              zstd_sys::ZSTD_createDDict_byReference(ptr_void(dict_buffer),
                                                     dict_buffer.len())
          },
          PhantomData)
}
pub fn get_dict_id_from_dict(dict: &[u8]) -> u32 {
    unsafe {
        zstd_sys::ZSTD_getDictID_fromDict(ptr_void(dict), dict.len()) as u32
    }
}
pub fn get_dict_id_from_ddict(ddict: &DDict) -> u32 {
    unsafe { zstd_sys::ZSTD_getDictID_fromDDict(ddict.0) as u32 }
}
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
pub fn get_block_size_max(cctx: &mut CCtx) -> usize {
    unsafe { zstd_sys::ZSTD_getBlockSizeMax(cctx.0) }
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
