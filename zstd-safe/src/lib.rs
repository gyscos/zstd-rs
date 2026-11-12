#![no_std]
//! Minimal safe wrapper around zstd-sys.
//!
//! This crates provides a minimal translation of the [zstd-sys] methods.
//! For a more comfortable high-level library, see the [zstd] crate.
//!
//! [zstd-sys]: https://crates.io/crates/zstd-sys
//! [zstd]: https://crates.io/crates/zstd
//!
//! Most of the functions here map 1-for-1 to a function from
//! [the C zstd library][zstd-c] mentioned in their descriptions.
//! Check the [source documentation][doc] for more information on their
//! behaviour.
//!
//! [doc]: https://facebook.github.io/zstd/zstd_manual.html
//! [zstd-c]: https://facebook.github.io/zstd/
//!
//! Features denoted as experimental in the C library are hidden behind an
//! `experimental` feature.
#![cfg_attr(feature = "doc-cfg", feature(doc_cfg))]

#[cfg(feature = "alloc")]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

#[cfg(test)]
mod tests;

#[cfg(feature = "with-rust-allocator")]
mod rust_allocator;

#[cfg(feature = "seekable")]
pub mod seekable;

// Re-export zstd-sys
pub use zstd_sys;

/// How to compress data.
pub use zstd_sys::ZSTD_strategy as Strategy;

/// Frame progression state.
#[cfg(feature = "experimental")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
pub use zstd_sys::ZSTD_frameProgression as FrameProgression;

/// Reset directive.
// pub use zstd_sys::ZSTD_ResetDirective as ResetDirective;
use core::ffi::{c_char, c_int, c_ulonglong, c_void};

use core::marker::PhantomData;
use core::num::{NonZeroU32, NonZeroU64};
use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::str;

include!("constants.rs");

#[cfg(feature = "experimental")]
include!("constants_experimental.rs");

#[cfg(feature = "seekable")]
include!("constants_seekable.rs");

/// Represents the compression level used by zstd.
pub type CompressionLevel = i32;

/// Represents a possible error from the zstd library.
pub type ErrorCode = usize;

/// Wrapper result around most zstd functions.
///
/// Either a success code (usually number of bytes written), or an error code.
pub type SafeResult = Result<usize, ErrorCode>;

/// The error code zstd itself returns when an allocation fails
/// (`-ZSTD_error_memory_allocation` as a `size_t`), for the places where a
/// context is allocated on the Rust side of a call.
#[cfg(feature = "with-rust-allocator")]
const MEMORY_ALLOCATION_ERROR: ErrorCode =
    (zstd_sys::ZSTD_ErrorCode::ZSTD_error_memory_allocation as usize)
        .wrapping_neg();

/// The error code zstd itself returns for a parameter it does not support
/// (`-ZSTD_error_parameter_unsupported` as a `size_t`), for the parameters
/// that are refused on the Rust side of a call.
#[cfg(feature = "experimental")]
const PARAMETER_UNSUPPORTED_ERROR: ErrorCode =
    (zstd_sys::ZSTD_ErrorCode::ZSTD_error_parameter_unsupported as usize)
        .wrapping_neg();

/// The error code zstd itself returns for an operation the context is in no
/// state to perform (`-ZSTD_error_stage_wrong` as a `size_t`), for the places
/// where that has to be decided on the Rust side of a call.
const STAGE_WRONG_ERROR: ErrorCode =
    (zstd_sys::ZSTD_ErrorCode::ZSTD_error_stage_wrong as usize).wrapping_neg();

/// The error code zstd returns for a dictionary that does not fit
/// (`-ZSTD_error_dictionary_wrong`), for `CCtx::compress_with_*` on a context
/// with a loaded dictionary, which they would lose.
const DICTIONARY_WRONG_ERROR: ErrorCode =
    (zstd_sys::ZSTD_ErrorCode::ZSTD_error_dictionary_wrong as usize)
        .wrapping_neg();

/// The error code `ZSTD_compress2()` returns when the destination is too
/// small, leaving its frame in progress (`-ZSTD_error_dstSize_tooSmall`).
const DST_SIZE_TOO_SMALL_ERROR: ErrorCode =
    (zstd_sys::ZSTD_ErrorCode::ZSTD_error_dstSize_tooSmall as usize)
        .wrapping_neg();

/// The largest frame zstd compresses on the calling thread when its size is
/// known (`ZSTDMT_JOBSIZE_MIN` in zstd 1.5; older versions used larger
/// values, which only makes this cautious).
const ZSTDMT_JOBSIZE_MIN: usize = 512 << 10;

/// Indicates an error happened when parsing the frame content size.
///
/// The stream may be corrupted, or the given frame prefix was too small.
#[derive(Debug)]
pub struct ContentSizeError;

impl core::fmt::Display for ContentSizeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Could not get content size")
    }
}

/// Returns true if code represents error.
fn is_error(code: usize) -> bool {
    // Safety: Just FFI
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

/// Parse a content size value.
///
/// zstd uses 2 special content size values to indicate either unknown size or parsing error.
fn parse_content_size(
    content_size: u64,
) -> Result<Option<u64>, ContentSizeError> {
    match content_size {
        CONTENTSIZE_ERROR => Err(ContentSizeError),
        CONTENTSIZE_UNKNOWN => Ok(None),
        other => Ok(Some(other)),
    }
}

fn ptr_void(src: &[u8]) -> *const c_void {
    src.as_ptr() as *const c_void
}

fn ptr_mut_void(dst: &mut (impl WriteBuf + ?Sized)) -> *mut c_void {
    dst.as_mut_ptr() as *mut c_void
}

/// Returns the ZSTD version.
///
/// Returns `major * 10_000 + minor * 100 + patch`.
/// So 1.5.3 would be returned as `10_503`.
pub fn version_number() -> u32 {
    // Safety: Just FFI
    unsafe { zstd_sys::ZSTD_versionNumber() as u32 }
}

/// Returns a string representation of the ZSTD version.
///
/// For example "1.5.3".
pub fn version_string() -> &'static str {
    // Safety: Assumes `ZSTD_versionString` returns a valid utf8 string.
    unsafe { c_char_to_str(zstd_sys::ZSTD_versionString()) }
}

/// Returns the minimum (fastest) compression level supported.
///
/// This is likely going to be a _very_ large negative number.
pub fn min_c_level() -> CompressionLevel {
    // Safety: Just FFI
    unsafe { zstd_sys::ZSTD_minCLevel() as CompressionLevel }
}

/// Returns the maximum (slowest) compression level supported.
pub fn max_c_level() -> CompressionLevel {
    // Safety: Just FFI
    unsafe { zstd_sys::ZSTD_maxCLevel() as CompressionLevel }
}

/// Wraps the `ZSTD_compress` function.
///
/// This will try to compress `src` entirely and write the result to `dst`, returning the number of
/// bytes written. If `dst` is too small to hold the compressed content, an error will be returned.
///
/// For streaming operations that don't require to store the entire input/output in memory, see
/// `compress_stream`.
pub fn compress<C: WriteBuf + ?Sized>(
    dst: &mut C,
    src: &[u8],
    compression_level: CompressionLevel,
) -> SafeResult {
    // `ZSTD_compress` allocates its workspace with the C runtime's `malloc`.
    // Route it through a context built on the Rust global allocator instead;
    // `ZSTD_compressCCtx` on a fresh context is exactly what `ZSTD_compress`
    // does internally.
    #[cfg(feature = "with-rust-allocator")]
    {
        let mut cctx = CCtx::try_create().ok_or(MEMORY_ALLOCATION_ERROR)?;
        cctx.set_parameter(CParameter::CompressionLevel(compression_level))?;
        cctx.compress(dst, src)
    }

    // Safety: ZSTD_compress indeed returns how many bytes have been written.
    #[cfg(not(feature = "with-rust-allocator"))]
    unsafe {
        dst.write_from(|buffer, capacity| {
            parse_code(zstd_sys::ZSTD_compress(
                buffer,
                capacity,
                ptr_void(src),
                src.len(),
                compression_level,
            ))
        })
    }
}

/// Wraps the `ZSTD_decompress` function.
///
/// This is a one-step decompression (not streaming).
///
/// You will need to make sure `dst` is large enough to store all the decompressed content, or an
/// error will be returned.
///
/// If decompression was a success, the number of bytes written will be returned.
///
/// Only that many bytes at the start of `dst` are meaningful: zstd may use the
/// rest of `dst` as scratch space, so it can be overwritten even if it was
/// zero-filled beforehand.
///
/// With a `Vec`, the output goes into its spare capacity, replacing anything
/// already in it, and its length is then set to the number of bytes written.
/// The `Vec` is never re-allocated, so create it with `Vec::with_capacity` (or
/// `reserve`) for the full decompressed size: its capacity is what bounds the
/// output.
pub fn decompress<C: WriteBuf + ?Sized>(
    dst: &mut C,
    src: &[u8],
) -> SafeResult {
    // See `compress()`: keep the implicit context's allocations visible to
    // the Rust global allocator.
    #[cfg(feature = "with-rust-allocator")]
    {
        let mut dctx = DCtx::try_create().ok_or(MEMORY_ALLOCATION_ERROR)?;
        dctx.decompress(dst, src)
    }

    // Safety: ZSTD_decompress indeed returns how many bytes have been written.
    #[cfg(not(feature = "with-rust-allocator"))]
    unsafe {
        dst.write_from(|buffer, capacity| {
            parse_code(zstd_sys::ZSTD_decompress(
                buffer,
                capacity,
                ptr_void(src),
                src.len(),
            ))
        })
    }
}

/// Wraps the `ZSTD_getDecompressedSize` function.
///
/// Returns `None` if the size could not be found, or if the content is actually empty.
#[deprecated(note = "Use get_frame_content_size instead.")]
pub fn get_decompressed_size(src: &[u8]) -> Option<NonZeroU64> {
    // Safety: Just FFI
    NonZeroU64::new(unsafe {
        zstd_sys::ZSTD_getDecompressedSize(ptr_void(src), src.len()) as u64
    })
}

/// Maximum compressed size in worst case single-pass scenario
pub fn compress_bound(src_size: usize) -> usize {
    // Safety: Just FFI
    unsafe { zstd_sys::ZSTD_compressBound(src_size) }
}

/// Remembers an error that may have left a context in a state zstd considers
/// undefined.
///
/// zstd.h says of both `ZSTD_compressStream2()` and `ZSTD_decompressStream()`
/// that "if an operation ends with an error, it may leave [the context] in an
/// undefined state", and that calling them again on such a state is undefined
/// behaviour - the context has to be reset first. So once a streaming
/// operation fails, refuse to run another one until something resets the
/// context, and hand the original error back instead.
#[derive(Clone, Debug, Default)]
struct Poison(Option<ErrorCode>);

impl Poison {
    /// Fails with the error that poisoned the context, if there was one.
    fn guard(&self) -> Result<(), ErrorCode> {
        match self.0 {
            Some(code) => Err(code),
            None => Ok(()),
        }
    }

    /// Remembers `res` if it failed, and passes it through.
    fn record(&mut self, res: SafeResult) -> SafeResult {
        if let Err(code) = res {
            self.0 = Some(code);
        }
        res
    }

    /// The context was reset, so it is usable again.
    fn clear(&mut self) {
        self.0 = None;
    }
}

/// What a `CCtx` knows about its worker threads (`NbWorkers`).
#[derive(Clone, Copy, Debug, Default)]
struct Workers {
    /// `NbWorkers` was ever set to a non-zero value, see `guard_own_tables`.
    ever: bool,
    /// The `NbWorkers` asked for with `set_parameter` (0: none).
    ///
    /// Each frame gets these workers only if everything it reads is owned by
    /// the context or `'static`, see `CCtx::start_frame()`. A frame that
    /// reads borrowed data runs on the calling thread instead: a context can
    /// be leaked mid-frame, and its workers would then read the data after
    /// the borrow ended.
    count: u32,
    /// `NbWorkers` is currently set to a non-zero value in zstd: `count`,
    /// unless the frame in progress, or the last one, reads borrowed data.
    requested: bool,
    /// A borrowed prefix (`ref_prefix`) was referenced, and no frame has
    /// started since to consume it.
    prefix_borrowed: bool,
    /// A dictionary that is only borrowed is referenced (`ref_cdict`).
    dict_borrowed: bool,
    /// zstd holds a copy of a dictionary (`load_dictionary`), which nothing
    /// could load again after `compress_with_*` replaced it.
    loaded: bool,
    /// The `pending` frame reads borrowed data, so it runs on the calling
    /// thread (or, for a one-shot call, is waited for before returning).
    frame_borrows: bool,
    /// A frame was started while `NbWorkers` was set, and is not finished.
    ///
    /// Its jobs may be running on the worker threads, reading the context's
    /// dictionary and job table. zstd up to 1.5.7 does not wait for them when
    /// the frame is abandoned: not on a session reset, not before a short
    /// frame (which it compresses on the calling thread), and not before
    /// freeing the dictionary when the context is freed. So the frame is
    /// drained first, see `CCtx::drain()`.
    pending: bool,
    /// `ZSTD_e_end` was given for the `pending` frame. zstd then answers
    /// `ZSTD_e_continue` by resetting the session without waiting for the
    /// jobs, so that is refused here first.
    ended: bool,
    /// A `ZSTD_e_end` call took all of its input: zstd made the frame's last
    /// job. It would hand any new input to a worker as a job after the last
    /// one, which waits forever, so that is refused here first.
    last: bool,
    /// The size pledged for the next frame is short (see `short`).
    pledged_short: bool,
    /// The `pending` frame is known to be compressed on the calling thread:
    /// its size is known (pledged, or given whole with `ZSTD_e_end`), and is
    /// short (zstd hands out no jobs for a frame of at most
    /// `ZSTDMT_JOBSIZE_MIN`).
    short: bool,
    /// The `pending` frame may have handed jobs to the worker threads: it
    /// took some input, and is not `short`.
    jobs: bool,
    /// A job failed. zstd up to 1.5.7 can then keep a job it had prepared but
    /// not handed to a worker yet (`jobReady`), after clearing it: the next
    /// multithreaded frame hands that empty job to a worker, which crashes.
    /// That stays in the context's multithreading state, which only freeing
    /// the context frees, so no other multithreaded frame is started on it.
    broken: bool,
    /// `StableOutBuffer` is enabled. Draining needs an output buffer of its
    /// own, which zstd then refuses, so it is not allowed with workers.
    stable_out: bool,
}

/// Compression context
///
/// It is recommended to allocate a single context per thread and re-use it
/// for many compression operations.
///
/// # Worker threads and leaks
///
/// With `CParameter::NbWorkers`, the jobs of a frame run on worker threads,
/// which read the dictionary. Anything that abandons a frame in progress
/// waits for them, dropping the context included (see
/// [`CCtx::wait_for_workers()`]). A context that is leaked instead
/// (`mem::forget`, `Box::leak`, a reference cycle) never waits: with a
/// dictionary or prefix it only *borrows* ([`CCtx::ref_cdict()`],
/// [`CCtx::ref_prefix()`]), the workers may then read it after it is
/// dropped. Do not leak a context that is compressing with worker threads
/// from borrowed data. A dictionary loaded with
/// [`CCtx::load_dictionary()`] is copied, and leaks with the context.
pub struct CCtx<'a>(
    NonNull<zstd_sys::ZSTD_CCtx>,
    PhantomData<&'a ()>,
    Poison,
    Workers,
    Held,
);

/// What a `CCtx` keeps alive for zstd: the dictionary or prefix it
/// references, when the context owns it rather than borrowing it. Dropped
/// after `Drop for CCtx` waited for the workers, and leaked with the context.
#[derive(Default)]
struct Held {
    cdict: Option<CDict>,
    #[cfg(feature = "alloc")]
    prefix: Option<Owner>,
}

impl Default for CCtx<'_> {
    fn default() -> Self {
        CCtx::create()
    }
}

impl<'a> CCtx<'a> {
    /// Tries to create a new context.
    ///
    /// Returns `None` if zstd returns a NULL pointer - may happen if allocation fails.
    ///
    /// With the `with-rust-allocator` feature enabled, the context's internal
    /// allocations go through Rust's global allocator instead of the C
    /// runtime's `malloc`.
    pub fn try_create() -> Option<Self> {
        #[cfg(feature = "with-rust-allocator")]
        // Safety: the callbacks satisfy zstd's allocation contract and remain
        // valid for the lifetime of the context.
        let context = unsafe {
            zstd_sys::ZSTD_createCCtx_advanced(
                crate::rust_allocator::RUST_GLOBAL_ALLOCATOR,
            )
        };
        #[cfg(not(feature = "with-rust-allocator"))]
        // Safety: zstd returns a newly owned context or null.
        let context = unsafe { zstd_sys::ZSTD_createCCtx() };

        Some(Self(
            NonNull::new(context)?,
            PhantomData,
            Poison::default(),
            Workers::default(),
            Held::default(),
        ))
    }

    /// Wrap `ZSTD_createCCtx`
    ///
    /// # Panics
    ///
    /// If zstd returns a NULL pointer.
    pub fn create() -> Self {
        Self::try_create()
            .expect("zstd returned null pointer when creating new context")
    }

    /// Compresses `src` into `dst` as one frame, with the parameters and the
    /// dictionary or prefix set on this context.
    ///
    /// This returns only once the frame is done, so it uses the worker
    /// threads of `NbWorkers` even when the frame reads borrowed data.
    ///
    /// Wraps the `ZSTD_compress2()` function.
    pub fn compress<C: WriteBuf + ?Sized>(
        &mut self,
        dst: &mut C,
        src: &[u8],
    ) -> SafeResult {
        if self.3.count != 0 {
            // Abandons the frame in progress, if any, waiting for its jobs.
            // Waiting may find that one of them failed: `start_frame` checks
            // `broken` after.
            self.reset(ResetDirective::SessionOnly)?;
            // This call returns only once its frame is done (see below), so
            // it may use the workers even when it reads borrowed data.
            self.start_frame(true)?;
        }
        if !self.3.requested {
            // Safety: ZSTD_compress2 returns how many bytes were written.
            let res = unsafe {
                dst.write_from(|buffer, capacity| {
                    parse_code(zstd_sys::ZSTD_compress2(
                        self.0.as_ptr(),
                        buffer,
                        capacity,
                        ptr_void(src),
                        src.len(),
                    ))
                })
            };
            if res.is_ok() {
                // The frame consumed the prefix, if any.
                self.3.prefix_borrowed = false;
            }
            // Unlike the others, this starts by resetting the session, and
            // ends it when it succeeds. When it fails, it stays mid-frame:
            // make streaming calls wait for an explicit reset.
            self.2.clear();
            return self.2.record(res);
        }

        // With worker threads, a frame this leaves in progress must be one
        // that can be waited for later, see `drain()`. `ZSTD_compress2()`
        // runs its frame with stable buffers, and zstd would then refuse any
        // other buffer: do what it does, but with ordinary ones. That costs
        // nothing to a multithreaded frame, which copies its input and output
        // anyway. A short frame, compressed on the calling thread, goes
        // straight to the destination when it can hold the whole frame, and
        // through a copy otherwise.
        //
        // A frame that reads borrowed data must not be left in progress: its
        // jobs would outlive the call, and the borrow with them if the
        // context is then leaked. So when the destination is too small, the
        // frame is waited for right away, before `write_from` runs any more
        // of the caller's `WriteBuf` code (which may panic).
        let mut raw = Ok(0);
        let mut taken = 0;
        let mut waited = false;
        // Safety: ZSTD_compressStream2 reports how many bytes it wrote in
        // `output.pos`.
        let res = unsafe {
            dst.write_from(|buffer, capacity| {
                let mut output = zstd_sys::ZSTD_outBuffer {
                    dst: buffer,
                    size: capacity,
                    pos: 0,
                };
                let mut input = zstd_sys::ZSTD_inBuffer {
                    src: ptr_void(src),
                    size: src.len(),
                    pos: 0,
                };
                raw = parse_code(zstd_sys::ZSTD_compressStream2(
                    self.0.as_ptr(),
                    &mut output,
                    &mut input,
                    zstd_sys::ZSTD_EndDirective::ZSTD_e_end,
                ));
                taken = input.pos;
                match raw {
                    Ok(0) => Ok(output.pos),
                    Ok(_) if self.3.frame_borrows => {
                        waited = true;
                        if let Err(code) = self.flush_to_scratch() {
                            raw = Err(code);
                        }
                        Err(DST_SIZE_TOO_SMALL_ERROR)
                    }
                    // As `ZSTD_compress2()`: the frame stays in progress.
                    Ok(_) => Err(DST_SIZE_TOO_SMALL_ERROR),
                    Err(code) => Err(code),
                }
            })
        };
        self.track_frame(
            raw,
            zstd_sys::ZSTD_EndDirective::ZSTD_e_end,
            src.len(),
            taken,
        );
        if waited {
            // Nothing is left running: the frame is over, but not its
            // session, which `ZSTD_compress2()` would also leave open.
            self.end_frame();
        }
        self.2.record(res)
    }

    /// Compresses `src` into `dst` as one frame, with `cdict` for this call
    /// only, and the other parameters set on this context.
    ///
    /// The dictionary is only borrowed, and nothing keeps it once this
    /// returns: unlike [`CCtx::ref_cdict()`], this takes no handle (no
    /// reference count to update), and leaves no dictionary alive in the
    /// context. Like [`CCtx::compress()`], it uses the worker threads.
    ///
    /// A dictionary the context holds ([`CCtx::ref_cdict()`],
    /// [`CCtx::set_cdict()`]) is referenced again afterwards; one it only
    /// borrows, or a prefix, is not. With a dictionary loaded with
    /// [`CCtx::load_dictionary()`], which could not be loaded again, this
    /// fails with a `dictionary_wrong` error: disable it first.
    pub fn compress_with_cdict<C: WriteBuf + ?Sized>(
        &mut self,
        dst: &mut C,
        src: &[u8],
        cdict: &CDict,
    ) -> SafeResult {
        // Safety: Just FFI. `compress_with` stops zstd referencing it.
        self.compress_with(dst, src, false, |cctx| unsafe {
            zstd_sys::ZSTD_CCtx_refCDict(cctx, cdict.as_ptr())
        })
    }

    /// Compresses `src` into `dst` as one frame, with `prefix` as its
    /// single-use dictionary, and the parameters set on this context.
    ///
    /// The prefix is only borrowed for this call. Like
    /// [`CCtx::compress()`], it uses the worker threads.
    ///
    /// A dictionary the context holds ([`CCtx::ref_cdict()`],
    /// [`CCtx::set_cdict()`]) is referenced again afterwards; one it only
    /// borrows, or another prefix, is not. With a dictionary loaded with
    /// [`CCtx::load_dictionary()`], which could not be loaded again, this
    /// fails with a `dictionary_wrong` error: disable it first.
    pub fn compress_with_prefix<C: WriteBuf + ?Sized>(
        &mut self,
        dst: &mut C,
        src: &[u8],
        prefix: &[u8],
    ) -> SafeResult {
        // Safety: Just FFI. `compress_with` stops zstd referencing it.
        self.compress_with(dst, src, true, |cctx| unsafe {
            zstd_sys::ZSTD_CCtx_refPrefix(cctx, ptr_void(prefix), prefix.len())
        })
    }

    /// `compress()` with data that `refer` references for this call only
    /// (a prefix when `prefix`, a dictionary otherwise).
    ///
    /// On every way out, panics included, the data is unreferenced before
    /// its borrow ends: the frame is waited for, the session reset, and what
    /// the context held referenced again (or nothing).
    fn compress_with<C: WriteBuf + ?Sized>(
        &mut self,
        dst: &mut C,
        src: &[u8],
        prefix: bool,
        refer: impl FnOnce(*mut zstd_sys::ZSTD_CCtx) -> usize,
    ) -> SafeResult {
        /// Puts the context back as it was, see `compress_with`.
        struct Restore<'c, 'a> {
            cctx: &'c mut CCtx<'a>,
            held: Option<CDict>,
        }
        impl Drop for Restore<'_, '_> {
            fn drop(&mut self) {
                let cctx = &mut *self.cctx;
                cctx.drain();
                // Safety: Just FFI. Between frames, zstd takes both.
                unsafe {
                    zstd_sys::ZSTD_CCtx_reset(
                        cctx.0.as_ptr(),
                        zstd_sys::ZSTD_ResetDirective::ZSTD_reset_session_only,
                    );
                    let held = self
                        .held
                        .as_ref()
                        .map_or(core::ptr::null_mut(), CDict::as_ptr);
                    zstd_sys::ZSTD_CCtx_refCDict(cctx.0.as_ptr(), held);
                }
                cctx.2.clear();
                cctx.3.pledged_short = false;
                cctx.referenced(false, false);
                cctx.4.cdict = self.held.take();
            }
        }

        if self.3.loaded {
            // Referencing anything else would drop it for good.
            return Err(DICTIONARY_WRONG_ERROR);
        }
        // zstd only takes a new reference between frames.
        self.reset(ResetDirective::SessionOnly)?;
        let held = core::mem::take(&mut self.4).cdict;
        let guard = Restore { cctx: self, held };
        parse_code(refer(guard.cctx.0.as_ptr()))?;
        guard.cctx.referenced(prefix, !prefix);
        guard.cctx.compress(dst, src)
    }

    /// Initializes the context with the given compression level.
    ///
    /// This is equivalent to running:
    /// * `reset()`
    /// * `set_parameter(CompressionLevel, compression_level)`
    ///
    /// Abandons the current frame, if any. With worker threads, this first
    /// waits for its jobs, see [`CCtx::wait_for_workers()`].
    pub fn init(&mut self, compression_level: CompressionLevel) -> SafeResult {
        // This also resets the session and unloads the dictionary.
        self.drain();
        self.2.clear();
        // Safety: Just FFI
        let code = unsafe {
            zstd_sys::ZSTD_initCStream(self.0.as_ptr(), compression_level)
        };
        let res = self.2.record(parse_code(code));
        if res.is_ok() {
            self.referenced(false, false);
        }
        res
    }

    /// Wraps the `ZSTD_initCStream_srcSize()` function.
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    #[deprecated(
        note = "Use reset, set_parameter, and set_pledged_src_size instead."
    )]
    pub fn init_src_size(
        &mut self,
        compression_level: CompressionLevel,
        pledged_src_size: u64,
    ) -> SafeResult {
        self.drain();
        self.2.clear();
        // Safety: Just FFI
        let code = unsafe {
            zstd_sys::ZSTD_initCStream_srcSize(
                self.0.as_ptr(),
                compression_level as c_int,
                pledged_src_size as c_ulonglong,
            )
        };
        let res = parse_code(code);
        if res.is_ok() {
            self.referenced(false, false);
        }
        res
    }

    /// Wraps the `ZSTD_initCStream_usingDict()` function.
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    #[deprecated(
        note = "Use reset, set_parameter, and load_dictionary instead."
    )]
    pub fn init_using_dict(
        &mut self,
        dict: &[u8],
        compression_level: CompressionLevel,
    ) -> SafeResult {
        self.drain();
        self.2.clear();
        // Safety: Just FFI
        let code = unsafe {
            zstd_sys::ZSTD_initCStream_usingDict(
                self.0.as_ptr(),
                ptr_void(dict),
                dict.len(),
                compression_level,
            )
        };
        let res = parse_code(code);
        if res.is_ok() {
            // The dictionary is copied.
            self.referenced(false, false);
            self.3.loaded = !dict.is_empty();
        }
        res
    }

    /// Wraps the `ZSTD_initCStream_usingCDict()` function.
    ///
    /// With `alloc`, the context keeps its own handle to the dictionary, see
    /// [`CCtx::ref_cdict()`].
    #[cfg(all(
        feature = "experimental",
        feature = "alloc",
        target_has_atomic = "ptr"
    ))]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    #[deprecated(note = "Use reset and ref_cdict instead.")]
    pub fn init_using_cdict(&mut self, cdict: &CDict) -> SafeResult {
        self.init_with_cdict(cdict, Some(cdict.clone()))
    }

    /// Wraps the `ZSTD_initCStream_usingCDict()` function.
    ///
    /// Dictionary must outlive the context.
    ///
    /// ```compile_fail,E0515
    /// # #![allow(deprecated)]
    /// fn compressor() -> zstd_safe::CCtx<'static> {
    ///     let cdict = zstd_safe::CDict::create(b"dictionary", 3);
    ///     let mut cctx = zstd_safe::CCtx::create();
    ///     cctx.init_using_cdict(&cdict).unwrap();
    ///     cctx // `cdict` would be dropped while `cctx` still points at it.
    /// }
    /// ```
    #[cfg(all(
        feature = "experimental",
        not(all(feature = "alloc", target_has_atomic = "ptr"))
    ))]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    #[deprecated(note = "Use reset and ref_cdict instead.")]
    pub fn init_using_cdict(&mut self, cdict: &'a CDict) -> SafeResult {
        self.init_with_cdict(cdict, None)
    }

    /// `init_using_cdict()`, keeping `held` (a handle to `cdict`, or `None`
    /// when it is only borrowed).
    #[cfg(feature = "experimental")]
    fn init_with_cdict(
        &mut self,
        cdict: &CDict,
        held: Option<CDict>,
    ) -> SafeResult {
        self.drain();
        self.2.clear();
        // Safety: Just FFI
        let code = unsafe {
            zstd_sys::ZSTD_initCStream_usingCDict(
                self.0.as_ptr(),
                cdict.as_ptr(),
            )
        };
        let res = self.2.record(parse_code(code));
        if res.is_ok() {
            self.hold_cdict(held);
        }
        res
    }

    /// Tries to load a dictionary.
    ///
    /// The dictionary content will be copied internally and does not need to be kept alive after
    /// calling this function.
    ///
    /// If you need to use the same dictionary for multiple contexts, it may be more efficient to
    /// create a `CDict` first, then loads that.
    ///
    /// The dictionary will apply to all compressed frames, until a new dictionary is set.
    ///
    /// Like zstd, this fails with a `stage_wrong` error while a frame is in
    /// progress: reset the session first (see [`CCtx::wait_for_workers()`]).
    pub fn load_dictionary(&mut self, dict: &[u8]) -> SafeResult {
        self.guard_workers()?;
        // Safety: Just FFI
        let res = parse_code(unsafe {
            zstd_sys::ZSTD_CCtx_loadDictionary(
                self.0.as_ptr(),
                ptr_void(dict),
                dict.len(),
            )
        });
        if res.is_ok() {
            // The dictionary is copied.
            self.referenced(false, false);
            self.3.loaded = !dict.is_empty();
        }
        res
    }

    /// References a prepared dictionary for the next frames.
    ///
    /// The context keeps its own handle to it, so it can come from anywhere
    /// (a cache, a `static`, a local), and the context needs no lifetime for
    /// it. Frames with it may use worker threads.
    ///
    /// The dictionary decides the compression parameters (its level, say);
    /// the others stay the context's.
    ///
    /// Like zstd, this fails with a `stage_wrong` error while a frame is in
    /// progress: reset the session first (see [`CCtx::wait_for_workers()`]).
    ///
    /// Wraps the `ZSTD_CCtx_refCDict()` function.
    #[cfg(all(feature = "alloc", target_has_atomic = "ptr"))]
    pub fn ref_cdict(&mut self, cdict: &CDict) -> SafeResult {
        self.set_cdict(cdict.clone())
    }

    /// References a prepared dictionary for the next frames, borrowing it.
    ///
    /// Without `alloc`, a dictionary cannot be shared otherwise. A streaming
    /// frame that reads a borrowed dictionary runs on the calling thread,
    /// see [`CCtx`](CCtx#worker-threads-and-leaks); [`CCtx::set_cdict()`]
    /// moves the dictionary into the context instead.
    ///
    /// Like zstd, this fails with a `stage_wrong` error while a frame is in
    /// progress: reset the session first (see [`CCtx::wait_for_workers()`]).
    ///
    /// Wraps the `ZSTD_CCtx_refCDict()` function.
    #[cfg(not(all(feature = "alloc", target_has_atomic = "ptr")))]
    pub fn ref_cdict(&mut self, cdict: &'a CDict) -> SafeResult {
        self.refer_cdict(cdict.as_ptr(), None)
    }

    /// References a prepared dictionary for the next frames, moving it into
    /// the context.
    ///
    /// See [`CCtx::ref_cdict()`].
    pub fn set_cdict(&mut self, cdict: CDict) -> SafeResult {
        self.refer_cdict(cdict.as_ptr(), Some(cdict))
    }

    /// `ref_cdict()` with zstd's pointer to the dictionary, keeping `held`
    /// (a handle to it, or `None` when it is only borrowed) once zstd
    /// accepted it.
    fn refer_cdict(
        &mut self,
        cdict: *mut zstd_sys::ZSTD_CDict,
        held: Option<CDict>,
    ) -> SafeResult {
        self.guard_workers()?;
        // Safety: Just FFI
        let res = parse_code(unsafe {
            zstd_sys::ZSTD_CCtx_refCDict(self.0.as_ptr(), cdict)
        });
        if res.is_ok() {
            self.hold_cdict(held);
        }
        res
    }

    /// Records that zstd now references a dictionary: the one `held` is a
    /// handle to, or a borrowed one.
    fn hold_cdict(&mut self, held: Option<CDict>) {
        self.referenced(false, held.is_none());
        self.4.cdict = held;
    }

    /// Return to "no-dictionary" mode.
    ///
    /// This will disable any dictionary/prefix previously registered for future frames.
    ///
    /// Like zstd, this fails with a `stage_wrong` error while a frame is in
    /// progress: reset the session first (see [`CCtx::wait_for_workers()`]).
    pub fn disable_dictionary(&mut self) -> SafeResult {
        self.guard_workers()?;
        // Safety: Just FFI
        let res = parse_code(unsafe {
            zstd_sys::ZSTD_CCtx_loadDictionary(
                self.0.as_ptr(),
                core::ptr::null(),
                0,
            )
        });
        if res.is_ok() {
            self.referenced(false, false);
        }
        res
    }

    /// Use some prefix as single-use dictionary for the next compressed frame.
    ///
    /// Just like a dictionary, decompression will need to be given the same prefix.
    ///
    /// This is best used if the "prefix" looks like the data to be compressed.
    /// With worker threads, do not leak the context, see
    /// [its documentation](CCtx#worker-threads-and-leaks).
    ///
    /// Like zstd, this fails with a `stage_wrong` error while a frame is in
    /// progress: reset the session first (see [`CCtx::wait_for_workers()`]).
    pub fn ref_prefix<'b>(&mut self, prefix: &'b [u8]) -> SafeResult
    where
        'b: 'a,
    {
        self.guard_workers()?;
        // Safety: Just FFI
        let res = parse_code(unsafe {
            zstd_sys::ZSTD_CCtx_refPrefix(
                self.0.as_ptr(),
                ptr_void(prefix),
                prefix.len(),
            )
        });
        if res.is_ok() {
            // An empty prefix references nothing.
            self.referenced(!prefix.is_empty(), false);
        }
        res
    }

    /// Uses a prefix as single-use dictionary for the next compressed frame,
    /// owning it.
    ///
    /// `prefix` gives the bytes without copying them: a `Vec<u8>`, an
    /// `Arc<[u8]>` or `Arc<Vec<u8>>`, a memory map, a `bytes::Bytes`, a
    /// `&'static [u8]`... The context keeps it until another dictionary or
    /// prefix replaces it, so a frame that reads it may use worker threads,
    /// unlike with [`CCtx::ref_prefix()`].
    ///
    /// Like zstd, this fails with a `stage_wrong` error while a frame is in
    /// progress: reset the session first (see [`CCtx::wait_for_workers()`]).
    #[cfg(feature = "alloc")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "alloc")))]
    pub fn set_prefix<P>(&mut self, prefix: P) -> SafeResult
    where
        P: core::ops::Deref + Send + Sync + 'static,
        P::Target: AsRef<[u8]>,
    {
        self.guard_workers()?;
        let (owner, ptr, len) = Owner::new(prefix);
        // Safety: Just FFI. The context owns the bytes from here on, and
        // keeps them until zstd no longer references them.
        let res = parse_code(unsafe {
            zstd_sys::ZSTD_CCtx_refPrefix(self.0.as_ptr(), ptr.cast(), len)
        });
        if res.is_ok() {
            self.referenced(false, false);
            self.4.prefix = Some(owner);
        }
        res
    }

    /// Performs a step of a streaming compression operation.
    ///
    /// This will read some data from `input` and/or write some data to `output`.
    ///
    /// The `end_op` directive can be used to specify what to do after: nothing special, flush
    /// internal buffers, or end the frame.
    ///
    /// # Returns
    ///
    /// An lower bound for the amount of data that still needs to be flushed out.
    ///
    /// This is useful when flushing or ending the frame: you need to keep calling this function
    /// until it returns 0.
    ///
    /// Wraps the `ZSTD_compressStream2()` function.
    pub fn compress_stream<C: WriteBuf + ?Sized>(
        &mut self,
        output: &mut OutBuffer<'_, C>,
        input: &mut InBuffer<'_>,
        end_op: zstd_sys::ZSTD_EndDirective,
    ) -> SafeResult {
        self.2.guard()?;
        if let Some(res) =
            self.before_stream(input.pos == input.src.len(), end_op)
        {
            return res;
        }
        let remaining = input.src.len().saturating_sub(input.pos);
        let mut output = output.wrap();
        let mut input = input.wrap();
        // Safety: Just FFI
        let code = unsafe {
            zstd_sys::ZSTD_compressStream2(
                self.0.as_ptr(),
                ptr_mut(&mut output),
                ptr_mut(&mut input),
                end_op,
            )
        };
        let taken = input.buf.pos.saturating_sub(input.parent.pos);
        let res = output.check(parse_code(code));
        let res = self.2.record(input.check(res));
        self.track_frame(res, end_op, remaining, taken);
        res
    }

    /// Flush any intermediate buffer.
    ///
    /// To fully flush, you should keep calling this function until it returns `Ok(0)`.
    ///
    /// Wraps the `ZSTD_flushStream()` function.
    pub fn flush_stream<C: WriteBuf + ?Sized>(
        &mut self,
        output: &mut OutBuffer<'_, C>,
    ) -> SafeResult {
        use zstd_sys::ZSTD_EndDirective::ZSTD_e_flush;
        self.2.guard()?;
        if let Some(res) = self.before_stream(true, ZSTD_e_flush) {
            return res;
        }
        let mut output = output.wrap();
        // Safety: Just FFI
        let code = unsafe {
            zstd_sys::ZSTD_flushStream(self.0.as_ptr(), ptr_mut(&mut output))
        };
        let res = self.2.record(output.check(parse_code(code)));
        self.track_frame(res, ZSTD_e_flush, 0, 0);
        res
    }

    /// Ends the stream.
    ///
    /// You should keep calling this function until it returns `Ok(0)`.
    ///
    /// Wraps the `ZSTD_endStream()` function.
    pub fn end_stream<C: WriteBuf + ?Sized>(
        &mut self,
        output: &mut OutBuffer<'_, C>,
    ) -> SafeResult {
        use zstd_sys::ZSTD_EndDirective::ZSTD_e_end;
        self.2.guard()?;
        if let Some(res) = self.before_stream(true, ZSTD_e_end) {
            return res;
        }
        let mut output = output.wrap();
        // Safety: Just FFI
        let code = unsafe {
            zstd_sys::ZSTD_endStream(self.0.as_ptr(), ptr_mut(&mut output))
        };
        let res = self.2.record(output.check(parse_code(code)));
        self.track_frame(res, ZSTD_e_end, 0, 0);
        res
    }

    /// Decides whether a streaming call may reach zstd, see `Workers`.
    ///
    /// Returns what the call should return instead when it may not.
    ///
    /// Without `NbWorkers`, this is one test, inlined in every streaming
    /// call: the rest is kept out of line.
    #[inline]
    fn before_stream(
        &mut self,
        no_input: bool,
        end_op: zstd_sys::ZSTD_EndDirective,
    ) -> Option<SafeResult> {
        if self.3.count == 0 {
            return None;
        }
        self.before_stream_with_workers(no_input, end_op)
    }

    /// `before_stream()` while `NbWorkers` is set.
    #[inline(never)]
    fn before_stream_with_workers(
        &mut self,
        no_input: bool,
        end_op: zstd_sys::ZSTD_EndDirective,
    ) -> Option<SafeResult> {
        use zstd_sys::ZSTD_EndDirective::{ZSTD_e_continue, ZSTD_e_end};
        if !self.3.pending {
            // This call starts a frame. A streaming frame can be left in
            // progress, so it only gets the workers without borrowed data.
            if let Err(code) = self.start_frame(false) {
                return Some(Err(code));
            }
        }
        if !self.3.requested {
            return None;
        }
        if self.3.pending {
            if (self.3.ended && end_op == ZSTD_e_continue)
                || (self.3.last && !no_input)
            {
                return Some(Err(STAGE_WRONG_ERROR));
            }
        } else if no_input && end_op != ZSTD_e_end {
            // Starting a frame with nothing in it. If it was then ended,
            // zstd up to 1.5.7 would hand an empty job to a worker, which its
            // own wait on errors (`ZSTDMT_waitForAllJobsCompleted`) takes for
            // done before the worker even starts. `drain()` does wait for it,
            // so this is only a safeguard: wait for some input, nothing would
            // have been written yet anyway. (A frame that starts with
            // `ZSTD_e_end` takes the size of its input, so an empty one is
            // compressed on the calling thread.)
            return Some(Ok(0));
        }
        None
    }

    /// Gives the frame about to start the workers it may use, see
    /// `Workers::count`.
    ///
    /// `blocking` is for a call that returns only once its frame is done:
    /// that frame may use the workers even when it reads borrowed data.
    /// Only called while `NbWorkers` is set and no frame is in progress, when
    /// zstd still takes a new `nbWorkers`.
    fn start_frame(&mut self, blocking: bool) -> Result<(), ErrorCode> {
        let borrows = self.3.prefix_borrowed || self.3.dict_borrowed;
        let workers = blocking || !borrows;
        if workers && self.3.broken {
            // See `Workers::broken`.
            return Err(STAGE_WRONG_ERROR);
        }
        if workers != self.3.requested {
            let value = if workers { self.3.count } else { 0 };
            // Safety: Just FFI
            parse_code(unsafe {
                zstd_sys::ZSTD_CCtx_setParameter(
                    self.0.as_ptr(),
                    zstd_sys::ZSTD_cParameter::ZSTD_c_nbWorkers,
                    value as c_int,
                )
            })?;
            self.3.requested = workers;
        }
        self.3.frame_borrows = borrows;
        Ok(())
    }

    /// Records what a streaming call left open, see `Workers`.
    ///
    /// `given` is how much input the call was given, and `taken` how much of
    /// it zstd took.
    ///
    /// Inlined like `before_stream()`, with the work for `NbWorkers` out of
    /// line.
    #[inline]
    fn track_frame(
        &mut self,
        res: SafeResult,
        end_op: zstd_sys::ZSTD_EndDirective,
        given: usize,
        taken: usize,
    ) {
        // zstd started a frame by now, which consumed the prefix, if any. (A
        // failed call may not have: the prefix then stays counted.)
        self.3.prefix_borrowed &= res.is_err();
        // A pledged size is for the frame this call started or continued.
        let pledged_short = core::mem::take(&mut self.3.pledged_short);
        if self.3.count != 0 {
            self.track_frame_with_workers(
                res,
                end_op,
                given,
                taken,
                pledged_short,
            );
        }
    }

    /// `track_frame()` while `NbWorkers` is set.
    #[inline(never)]
    fn track_frame_with_workers(
        &mut self,
        res: SafeResult,
        end_op: zstd_sys::ZSTD_EndDirective,
        given: usize,
        taken: usize,
        pledged_short: bool,
    ) {
        use zstd_sys::ZSTD_EndDirective::ZSTD_e_end;
        // From here on, frames are tracked even when they run on the calling
        // thread: `NbWorkers` may only change between frames.
        if !self.3.pending {
            // This call started the frame. Given whole, its size is the
            // input's, whatever was pledged.
            self.3.short = if end_op == ZSTD_e_end {
                given <= ZSTDMT_JOBSIZE_MIN
            } else {
                pledged_short
            };
        }
        self.3.jobs |= self.3.requested && taken > 0 && !self.3.short;
        match res {
            Ok(0) if end_op == ZSTD_e_end => self.end_frame(),
            Ok(_) => {
                self.3.pending = true;
                if end_op == ZSTD_e_end {
                    self.3.ended = true;
                    self.3.last |= taken == given;
                }
            }
            // When a job failed, zstd waited for the others and reset the
            // session (refused directives and buffers never get that far).
            // Otherwise, there are no jobs. Either way, nothing is left to
            // wait for.
            Err(_) => {
                self.3.broken |= self.3.jobs;
                self.end_frame();
            }
        }
    }

    /// Records that no frame is in progress anymore.
    fn end_frame(&mut self) {
        self.3.pending = false;
        self.3.ended = false;
        self.3.last = false;
        self.3.short = false;
        self.3.jobs = false;
        self.3.frame_borrows = false;
    }

    /// Waits for the jobs of the frame in progress, if any, so that it can
    /// be abandoned.
    ///
    /// Asking zstd to flush everything waits for every job to be done and
    /// its output flushed (here to a scratch buffer, and thrown away). This
    /// also hands a worker the job zstd prepared but could not hand out yet.
    /// That compresses what was left in zstd's input buffer as one more
    /// job: wasted work, but bounded by one job. Starting a new frame would
    /// avoid it, but zstd up to 1.5.7 can then hand that prepared job out
    /// after clearing it, and a worker crashes.
    fn drain(&mut self) {
        if !self.3.pending {
            return;
        }
        // A frame on the calling thread has no jobs to wait for.
        if self.3.requested {
            let _ = self.flush_to_scratch();
        }
        self.end_frame();
    }

    /// Flushes the frame in progress to a scratch buffer, throwing the output
    /// away, until every job is done. See `drain()`.
    fn flush_to_scratch(&mut self) -> Result<(), ErrorCode> {
        let mut scratch = [0u8; 4096];
        loop {
            let mut output = zstd_sys::ZSTD_outBuffer {
                dst: ptr_mut_void(&mut scratch[..]),
                size: scratch.len(),
                pos: 0,
            };
            let mut input = zstd_sys::ZSTD_inBuffer {
                src: ptr_void(&[]),
                size: 0,
                pos: 0,
            };
            // Safety: Just FFI, with buffers that live through the call.
            let code = unsafe {
                zstd_sys::ZSTD_compressStream2(
                    self.0.as_ptr(),
                    &mut output,
                    &mut input,
                    zstd_sys::ZSTD_EndDirective::ZSTD_e_flush,
                )
            };
            match parse_code(code) {
                Ok(0) => return Ok(()),
                Ok(_) => continue,
                // A job failed: see `track_frame`.
                Err(code) => {
                    self.3.broken |= self.3.jobs;
                    return Err(code);
                }
            }
        }
    }

    /// Fails while a frame is in progress (zstd refuses these calls then).
    ///
    /// zstd up to 1.5.7 would also take them after the session was reset
    /// while worker threads were still running jobs, freeing what they read:
    /// every reset here waits for them first, and this keeps the error the
    /// same whatever the library does.
    fn guard_workers(&self) -> Result<(), ErrorCode> {
        if self.3.pending {
            return Err(STAGE_WRONG_ERROR);
        }
        Ok(())
    }

    /// Records a change to the dictionary or prefix the next frames read,
    /// once zstd accepted it. Any of them replaces the previous one.
    ///
    /// Drops the dictionary handle the context held, if any: zstd no longer
    /// references it, and resets what pointed into it before its next frame
    /// reads anything (`ZSTD_reset_matchState`). zstd only takes such a
    /// change between frames.
    fn referenced(&mut self, prefix_borrowed: bool, dict_borrowed: bool) {
        self.3.prefix_borrowed = prefix_borrowed;
        self.3.dict_borrowed = dict_borrowed;
        self.3.loaded = false;
        self.4 = Held::default();
    }

    /// How many worker threads the frame in progress uses, or the next frame
    /// will use if none is in progress.
    ///
    /// This is the `NbWorkers` set with [`CCtx::set_parameter()`], unless
    /// that frame reads borrowed data ([`CCtx::ref_prefix()`],
    /// [`CCtx::ref_cdict()`]): a streaming frame then runs on the calling
    /// thread, see [`CCtx`](CCtx#worker-threads-and-leaks). `compress()`
    /// always uses the workers.
    pub fn frame_workers(&self) -> u32 {
        let workers = if self.3.pending {
            self.3.requested
        } else {
            !(self.3.prefix_borrowed || self.3.dict_borrowed)
        };
        if workers {
            self.3.count
        } else {
            0
        }
    }

    /// Abandons the current frame, if any, and waits for the worker threads
    /// to be done with it.
    ///
    /// With `CParameter::NbWorkers`, the jobs of a frame run on worker
    /// threads, reading the dictionary. Anything that abandons a frame in
    /// progress waits for them first: a session reset (and `init()`, which
    /// implies one), a one-shot compression, and dropping the context. This
    /// is the same as `reset(SessionOnly)`. Waiting first compresses what was
    /// left of the frame's input as one more job, and throws the result away.
    ///
    /// After a job failed on a worker thread (for instance because the frame
    /// was given more than its pledged size), the context refuses any
    /// streaming call or `compress()` while `NbWorkers` is set, and setting
    /// it again (with a `stage_wrong` error): zstd up to 1.5.7, the version
    /// bundled here, can leave a cleared job behind that the next
    /// multithreaded frame would hand to a worker, which crashes. Create a new
    /// context then, or set `NbWorkers` to 0.
    ///
    /// Once a `ZSTD_e_end` call took all of its input, a frame takes no more:
    /// that is refused with a `stage_wrong` error too, instead of the worker
    /// threads waiting forever for a job after the last one.
    pub fn wait_for_workers(&mut self) -> SafeResult {
        self.reset(ResetDirective::SessionOnly)
    }

    /// Returns the size currently used by this context.
    ///
    /// This may change over time.
    pub fn sizeof(&self) -> usize {
        // Safety: Just FFI
        unsafe { zstd_sys::ZSTD_sizeof_CCtx(self.0.as_ptr()) }
    }

    /// Resets the state of the context.
    ///
    /// Depending on the reset mode, it can reset the session, the parameters, or both.
    ///
    /// Resetting the session abandons the frame in progress, if any: with
    /// worker threads, this first waits for its jobs, see
    /// [`CCtx::wait_for_workers()`]. Like zstd, resetting only the parameters
    /// fails with a `stage_wrong` error while a frame is in progress.
    ///
    /// Wraps the `ZSTD_CCtx_reset()` function.
    pub fn reset(&mut self, reset: ResetDirective) -> SafeResult {
        if reset.resets_session() {
            self.drain();
        } else {
            self.guard_workers()?;
        }
        // Safety: Just FFI
        let res = parse_code(unsafe {
            zstd_sys::ZSTD_CCtx_reset(self.0.as_ptr(), reset.as_sys())
        });
        if res.is_ok() && reset.resets_session() {
            self.2.clear();
            self.3.pledged_short = false;
        }
        if res.is_ok() && !matches!(reset, ResetDirective::SessionOnly) {
            self.3.count = 0;
            self.3.requested = false;
            self.3.stable_out = false;
            // Resetting the parameters also unloads the dictionary.
            self.referenced(false, false);
        }
        res
    }

    /// Sets a compression parameter.
    ///
    /// Some of these parameters need to be set during de-compression as well.
    ///
    /// Like zstd, changing `NbWorkers` fails with a `stage_wrong` error while
    /// a frame is in progress, and so does setting it to a non-zero value
    /// after a multithreaded call failed (see [`CCtx::wait_for_workers()`]).
    /// `NbWorkers` and `StableOutBuffer` cannot be used together.
    pub fn set_parameter(&mut self, param: CParameter) -> SafeResult {
        match param {
            CParameter::NbWorkers(workers) => {
                // zstd would resize the job table the old jobs still use.
                self.guard_workers()?;
                if workers != 0 {
                    if self.3.broken {
                        return Err(STAGE_WRONG_ERROR);
                    }
                    #[cfg(feature = "experimental")]
                    if self.3.stable_out {
                        return Err(PARAMETER_UNSUPPORTED_ERROR);
                    }
                }
            }
            // Waiting for the workers needs an output buffer of its own.
            #[cfg(feature = "experimental")]
            CParameter::StableOutBuffer(true) if self.3.count != 0 => {
                return Err(PARAMETER_UNSUPPORTED_ERROR);
            }
            _ => (),
        }
        // TODO: Until bindgen properly generates a binding for this, we'll need to do it here.

        #[cfg(feature = "experimental")]
        use zstd_sys::ZSTD_cParameter::{
            ZSTD_c_experimentalParam1 as ZSTD_c_rsyncable,
            ZSTD_c_experimentalParam10 as ZSTD_c_stableOutBuffer,
            ZSTD_c_experimentalParam11 as ZSTD_c_blockDelimiters,
            ZSTD_c_experimentalParam12 as ZSTD_c_validateSequences,
            ZSTD_c_experimentalParam13 as ZSTD_c_useBlockSplitter,
            ZSTD_c_experimentalParam14 as ZSTD_c_useRowMatchFinder,
            ZSTD_c_experimentalParam15 as ZSTD_c_deterministicRefPrefix,
            ZSTD_c_experimentalParam16 as ZSTD_c_prefetchCDictTables,
            ZSTD_c_experimentalParam17 as ZSTD_c_enableSeqProducerFallback,
            ZSTD_c_experimentalParam18 as ZSTD_c_maxBlockSize,
            ZSTD_c_experimentalParam19 as ZSTD_c_searchForExternalRepcodes,
            ZSTD_c_experimentalParam2 as ZSTD_c_format,
            ZSTD_c_experimentalParam3 as ZSTD_c_forceMaxWindow,
            ZSTD_c_experimentalParam4 as ZSTD_c_forceAttachDict,
            ZSTD_c_experimentalParam5 as ZSTD_c_literalCompressionMode,
            ZSTD_c_experimentalParam7 as ZSTD_c_srcSizeHint,
            ZSTD_c_experimentalParam8 as ZSTD_c_enableDedicatedDictSearch,
            ZSTD_c_experimentalParam9 as ZSTD_c_stableInBuffer,
        };

        use zstd_sys::ZSTD_cParameter::*;
        use CParameter::*;

        let (param, value) = match param {
            #[cfg(feature = "experimental")]
            RSyncable(rsyncable) => (ZSTD_c_rsyncable, rsyncable as c_int),
            #[cfg(feature = "experimental")]
            Format(format) => (ZSTD_c_format, format as c_int),
            #[cfg(feature = "experimental")]
            ForceMaxWindow(force) => (ZSTD_c_forceMaxWindow, force as c_int),
            #[cfg(feature = "experimental")]
            ForceAttachDict(force) => (ZSTD_c_forceAttachDict, force as c_int),
            #[cfg(feature = "experimental")]
            LiteralCompressionMode(mode) => {
                (ZSTD_c_literalCompressionMode, mode as c_int)
            }
            #[cfg(feature = "experimental")]
            SrcSizeHint(value) => (ZSTD_c_srcSizeHint, value as c_int),
            #[cfg(feature = "experimental")]
            EnableDedicatedDictSearch(enable) => {
                (ZSTD_c_enableDedicatedDictSearch, enable as c_int)
            }
            // In stable mode zstd keeps reading from the `InBuffer` of one
            // call during `flush_stream`/`end_stream`, but the slice is only
            // borrowed for that one call: the safe API cannot uphold this.
            #[cfg(feature = "experimental")]
            StableInBuffer(true) => return Err(PARAMETER_UNSUPPORTED_ERROR),
            #[cfg(feature = "experimental")]
            StableInBuffer(false) => (ZSTD_c_stableInBuffer, 0),
            #[cfg(feature = "experimental")]
            StableOutBuffer(stable) => {
                (ZSTD_c_stableOutBuffer, stable as c_int)
            }
            #[cfg(feature = "experimental")]
            BlockDelimiters(value) => (ZSTD_c_blockDelimiters, value as c_int),
            #[cfg(feature = "experimental")]
            ValidateSequences(validate) => {
                (ZSTD_c_validateSequences, validate as c_int)
            }
            #[cfg(feature = "experimental")]
            UseBlockSplitter(split) => {
                (ZSTD_c_useBlockSplitter, split as c_int)
            }
            #[cfg(feature = "experimental")]
            UseRowMatchFinder(mode) => {
                (ZSTD_c_useRowMatchFinder, mode as c_int)
            }
            #[cfg(feature = "experimental")]
            DeterministicRefPrefix(deterministic) => {
                (ZSTD_c_deterministicRefPrefix, deterministic as c_int)
            }
            #[cfg(feature = "experimental")]
            PrefetchCDictTables(prefetch) => {
                (ZSTD_c_prefetchCDictTables, prefetch as c_int)
            }
            #[cfg(feature = "experimental")]
            EnableSeqProducerFallback(enable) => {
                (ZSTD_c_enableSeqProducerFallback, enable as c_int)
            }
            #[cfg(feature = "experimental")]
            MaxBlockSize(value) => (ZSTD_c_maxBlockSize, value as c_int),
            #[cfg(feature = "experimental")]
            SearchForExternalRepcodes(value) => {
                (ZSTD_c_searchForExternalRepcodes, value as c_int)
            }
            TargetCBlockSize(value) => {
                (ZSTD_c_targetCBlockSize, value as c_int)
            }
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
            LdmBucketSizeLog(value) => {
                (ZSTD_c_ldmBucketSizeLog, value as c_int)
            }
            LdmHashRateLog(value) => (ZSTD_c_ldmHashRateLog, value as c_int),
            ContentSizeFlag(flag) => (ZSTD_c_contentSizeFlag, flag as c_int),
            ChecksumFlag(flag) => (ZSTD_c_checksumFlag, flag as c_int),
            DictIdFlag(flag) => (ZSTD_c_dictIDFlag, flag as c_int),

            NbWorkers(value) => (ZSTD_c_nbWorkers, value as c_int),

            JobSize(value) => (ZSTD_c_jobSize, value as c_int),

            OverlapSizeLog(value) => (ZSTD_c_overlapLog, value as c_int),
        };

        // Safety: Just FFI
        let res = parse_code(unsafe {
            zstd_sys::ZSTD_CCtx_setParameter(self.0.as_ptr(), param, value)
        });
        if res.is_ok() && matches!(param, ZSTD_c_nbWorkers) {
            self.3.count = value as u32;
            self.3.requested = value != 0;
            self.3.ever |= value != 0;
        }
        #[cfg(feature = "experimental")]
        if res.is_ok() && matches!(param, ZSTD_c_stableOutBuffer) {
            self.3.stable_out = value != 0;
        }
        res
    }

    /// Guarantee that the input size will be this value.
    ///
    /// If given `None`, assumes the size is unknown.
    ///
    /// Unless explicitly disabled, this will cause the size to be written in the compressed frame
    /// header.
    ///
    /// If the actual data given to compress has a different size, an error will be returned.
    pub fn set_pledged_src_size(
        &mut self,
        pledged_src_size: Option<u64>,
    ) -> SafeResult {
        // Safety: Just FFI
        let res = parse_code(unsafe {
            zstd_sys::ZSTD_CCtx_setPledgedSrcSize(
                self.0.as_ptr(),
                pledged_src_size.unwrap_or(CONTENTSIZE_UNKNOWN) as c_ulonglong,
            )
        });
        if res.is_ok() {
            // See `Workers::short`.
            self.3.pledged_short = matches!(
                pledged_src_size,
                Some(size) if size <= ZSTDMT_JOBSIZE_MIN as u64
            );
        }
        res
    }

    /// Fails if the tables of this context may not be the ones its applied
    /// parameters describe.
    ///
    /// Starting a multi-threaded frame overwrites the applied parameters with
    /// those of the workers, but leaves the context's own stage and workspace
    /// as the last single-threaded frame left them. If that frame was
    /// abandoned before its first block, zstd still accepts the calls that
    /// work on this context directly, and overruns a workspace that was sized
    /// for other parameters. The applied parameters cannot be queried, so
    /// refuse these calls as soon as a multi-threaded frame could have been
    /// started.
    #[cfg(feature = "experimental")]
    fn guard_own_tables(&self) -> Result<(), ErrorCode> {
        if self.3.ever {
            return Err(STAGE_WRONG_ERROR);
        }
        Ok(())
    }

    /// Wraps the `ZSTD_getBlockSize()` function.
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    #[deprecated(
        note = "The block API is deprecated; use the normal compression API instead."
    )]
    pub fn get_block_size(&self) -> usize {
        // Safety: Just FFI
        unsafe { zstd_sys::ZSTD_getBlockSize(self.0.as_ptr()) }
    }

    /// Wraps the `ZSTD_compressBlock()` function.
    ///
    /// Returns an error once `NbWorkers` was set to a non-zero value on this
    /// context.
    ///
    /// # Safety
    ///
    /// `src` becomes this context's history window, so it must stay allocated and unmodified until
    /// the next call to `compress_block` on this context (or until this context is dropped), as the
    /// following block is compressed against it.
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    #[deprecated(
        note = "The block API is deprecated; use compress_stream2 instead."
    )]
    pub unsafe fn compress_block<C: WriteBuf + ?Sized>(
        &mut self,
        dst: &mut C,
        src: &[u8],
    ) -> SafeResult {
        self.guard_own_tables()?;
        // Safety: ZSTD_compressBlock returns the number of bytes written.
        unsafe {
            dst.write_from(|buffer, capacity| {
                parse_code(zstd_sys::ZSTD_compressBlock(
                    self.0.as_ptr(),
                    buffer,
                    capacity,
                    ptr_void(src),
                    src.len(),
                ))
            })
        }
    }

    /// Returns the recommended input buffer size.
    ///
    /// Using this size may result in minor performance boost.
    pub fn in_size() -> usize {
        // Safety: Just FFI
        unsafe { zstd_sys::ZSTD_CStreamInSize() }
    }

    /// Returns the recommended output buffer size.
    ///
    /// Using this may result in minor performance boost.
    pub fn out_size() -> usize {
        // Safety: Just FFI
        unsafe { zstd_sys::ZSTD_CStreamOutSize() }
    }

    /// Use a shared thread pool for this context.
    ///
    /// Thread pool must outlive the context.
    #[cfg(all(feature = "experimental", feature = "zstdmt"))]
    #[cfg_attr(
        feature = "doc-cfg",
        doc(cfg(all(feature = "experimental", feature = "zstdmt")))
    )]
    pub fn ref_thread_pool<'b>(&mut self, pool: &'b ThreadPool) -> SafeResult
    where
        'b: 'a,
    {
        parse_code(unsafe {
            zstd_sys::ZSTD_CCtx_refThreadPool(self.0.as_ptr(), pool.0.as_ptr())
        })
    }

    /// Return to using a private thread pool for this context.
    #[cfg(all(feature = "experimental", feature = "zstdmt"))]
    #[cfg_attr(
        feature = "doc-cfg",
        doc(cfg(all(feature = "experimental", feature = "zstdmt")))
    )]
    pub fn disable_thread_pool(&mut self) -> SafeResult {
        parse_code(unsafe {
            zstd_sys::ZSTD_CCtx_refThreadPool(
                self.0.as_ptr(),
                core::ptr::null_mut(),
            )
        })
    }

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    pub fn get_frame_progression(&self) -> FrameProgression {
        // Safety: Just FFI
        unsafe { zstd_sys::ZSTD_getFrameProgression(self.0.as_ptr()) }
    }
}

impl<'a> Drop for CCtx<'a> {
    fn drop(&mut self) {
        // zstd up to 1.5.7 frees the dictionary before it stops the worker
        // threads, and does not stop a shared pool's at all: wait for the
        // jobs of the frame in progress first.
        self.drain();
        // Safety: Just FFI
        unsafe {
            zstd_sys::ZSTD_freeCCtx(self.0.as_ptr());
        }
    }
}

// Safety: the context is a plain heap allocation this handle owns; zstd keeps
// no thread-local state for it, so it can be moved between threads.
unsafe impl Send for CCtx<'_> {}
// Safety: every method that mutates the context takes `&mut self`, so a shared
// `&CCtx` only ever reads. There is no interior mutability to race on.
unsafe impl Sync for CCtx<'_> {}

/// Converts a zstd-owned C string to a `str`.
///
/// # Safety
///
/// `text` must point to a nul-terminated C string that lives for the rest of
/// the program - which is what the zstd functions this is used with return,
/// as they hand back pointers to static string literals.
unsafe fn c_char_to_str(text: *const c_char) -> &'static str {
    // Safety: guaranteed by the caller, see above.
    core::ffi::CStr::from_ptr(text)
        .to_str()
        .expect("bad error message from zstd")
}

/// Returns the error string associated with an error code.
pub fn get_error_name(code: usize) -> &'static str {
    unsafe {
        // Safety: assumes ZSTD returns a well-formed utf8 string.
        let name = zstd_sys::ZSTD_getErrorName(code);
        c_char_to_str(name)
    }
}

/// Returns whether `code` is an error of the given kind.
///
/// The kinds are those of `zstd_sys::ZSTD_ErrorCode`, listed in
/// `zstd_errors.h`.
///
/// This goes from `kind` to a code rather than the other way around, so it
/// works whatever the version of the linked libzstd: a code these bindings
/// don't know is just not equal to any `kind`.
///
/// ```
/// use zstd_safe::zstd_sys::ZSTD_ErrorCode;
///
/// let mut small = [0u8; 4];
/// let code = zstd_safe::compress(&mut small[..], &[42u8; 1000], 3).unwrap_err();
/// assert!(zstd_safe::is_error_kind(code, ZSTD_ErrorCode::ZSTD_error_dstSize_tooSmall));
/// ```
pub fn is_error_kind(code: ErrorCode, kind: zstd_sys::ZSTD_ErrorCode) -> bool {
    // Error codes are the negated error values, as a `size_t`.
    is_error(code) && code == (kind as usize).wrapping_neg()
}

/// A Decompression Context.
///
/// The lifetime references the potential dictionary used for this context.
///
/// If no dictionary was used, it will most likely be `'static`.
///
/// Same as `DStream`.
pub struct DCtx<'a>(
    NonNull<zstd_sys::ZSTD_DCtx>,
    PhantomData<&'a ()>,
    Poison,
    DState,
);

/// What a `DCtx` remembers about its own use.
#[derive(Clone, Copy, Debug, Default)]
struct DState {
    /// A streaming (or block) call ran since the session was last reset, so
    /// a stream may be open, see `finish_one_shot`.
    streamed: bool,
}

impl Default for DCtx<'_> {
    fn default() -> Self {
        DCtx::create()
    }
}

impl<'a> DCtx<'a> {
    /// Try to create a new decompression context.
    ///
    /// Returns `None` if the operation failed (for example, not enough memory).
    ///
    /// With the `with-rust-allocator` feature enabled, the context's internal
    /// allocations go through Rust's global allocator instead of the C
    /// runtime's `malloc`.
    pub fn try_create() -> Option<Self> {
        #[cfg(feature = "with-rust-allocator")]
        // Safety: the callbacks satisfy zstd's allocation contract and remain
        // valid for the lifetime of the context.
        let context = unsafe {
            zstd_sys::ZSTD_createDCtx_advanced(
                crate::rust_allocator::RUST_GLOBAL_ALLOCATOR,
            )
        };
        #[cfg(not(feature = "with-rust-allocator"))]
        // Safety: zstd returns a newly owned context or null.
        let context = unsafe { zstd_sys::ZSTD_createDCtx() };

        Some(Self(
            NonNull::new(context)?,
            PhantomData,
            Poison::default(),
            DState::default(),
        ))
    }

    /// Creates a new decoding context.
    ///
    /// # Panics
    ///
    /// If the context creation fails.
    pub fn create() -> Self {
        Self::try_create()
            .expect("zstd returned null pointer when creating new context")
    }

    /// Ends the session of a one-shot decompression, and passes `res` through.
    ///
    /// zstd.h has these calls imply a reset, but they leave the stream stage
    /// alone. A stream they interrupted would then carry on from a state that
    /// still points at buffers and dictionaries borrowed for that one call. So
    /// reset explicitly: the next stream starts from scratch.
    ///
    /// Only an open stream can carry on, though: a new one starts by setting
    /// everything up again from the context's own dictionary. So when no
    /// streaming call ran since the last reset, there is nothing to end, and
    /// skipping the reset saves a call per frame - which shows on tiny ones.
    ///
    /// Only the check is inlined: it runs once per frame.
    #[inline]
    fn finish_one_shot(&mut self, res: SafeResult) -> SafeResult {
        if self.3.streamed || self.2.guard().is_err() {
            self.reset_after_one_shot(res)
        } else {
            res
        }
    }

    #[cold]
    #[inline(never)]
    fn reset_after_one_shot(&mut self, res: SafeResult) -> SafeResult {
        self.reset(ResetDirective::SessionOnly).and(res)
    }

    /// Fully decompress the given frame.
    ///
    /// This decompress an entire frame in-memory. If you can have enough memory to store both the
    /// input and output buffer, then it may be faster that streaming decompression.
    ///
    /// Only as many bytes as returned at the start of `dst` are meaningful:
    /// zstd may overwrite the rest. A `Vec` avoids this, see
    /// [`crate::decompress()`]: it is never re-allocated, so create it with
    /// `Vec::with_capacity` for the full decompressed size.
    ///
    /// Wraps the `ZSTD_decompressDCtx()` function.
    pub fn decompress<C: WriteBuf + ?Sized>(
        &mut self,
        dst: &mut C,
        src: &[u8],
    ) -> SafeResult {
        let res = unsafe {
            dst.write_from(|buffer, capacity| {
                parse_code(zstd_sys::ZSTD_decompressDCtx(
                    self.0.as_ptr(),
                    buffer,
                    capacity,
                    ptr_void(src),
                    src.len(),
                ))
            })
        };
        self.finish_one_shot(res)
    }

    /// Fully decompress the given frame using a dictionary.
    ///
    /// Dictionary must be identical to the one used during compression.
    ///
    /// If you plan on using the same dictionary multiple times, it is faster to create a `DDict`
    /// first and use `decompress_using_ddict`.
    ///
    /// Only as many bytes as returned at the start of `dst` are meaningful:
    /// zstd may overwrite the rest. A `Vec` avoids this, see
    /// [`crate::decompress()`]: it is never re-allocated, so create it with
    /// `Vec::with_capacity` for the full decompressed size.
    ///
    /// Wraps `ZSTD_decompress_usingDict`
    pub fn decompress_using_dict<C: WriteBuf + ?Sized>(
        &mut self,
        dst: &mut C,
        src: &[u8],
        dict: &[u8],
    ) -> SafeResult {
        let res = unsafe {
            dst.write_from(|buffer, capacity| {
                parse_code(zstd_sys::ZSTD_decompress_usingDict(
                    self.0.as_ptr(),
                    buffer,
                    capacity,
                    ptr_void(src),
                    src.len(),
                    ptr_void(dict),
                    dict.len(),
                ))
            })
        };
        self.finish_one_shot(res)
    }

    /// Fully decompress the given frame using a dictionary.
    ///
    /// Dictionary must be identical to the one used during compression.
    ///
    /// Only as many bytes as returned at the start of `dst` are meaningful:
    /// zstd may overwrite the rest. A `Vec` avoids this, see
    /// [`crate::decompress()`]: it is never re-allocated, so create it with
    /// `Vec::with_capacity` for the full decompressed size.
    ///
    /// Wraps the `ZSTD_decompress_usingDDict()` function.
    pub fn decompress_using_ddict<C: WriteBuf + ?Sized>(
        &mut self,
        dst: &mut C,
        src: &[u8],
        ddict: &DDict<'_>,
    ) -> SafeResult {
        let res = unsafe {
            dst.write_from(|buffer, capacity| {
                parse_code(zstd_sys::ZSTD_decompress_usingDDict(
                    self.0.as_ptr(),
                    buffer,
                    capacity,
                    ptr_void(src),
                    src.len(),
                    ddict.0.as_ptr(),
                ))
            })
        };
        self.finish_one_shot(res)
    }

    /// Initializes an existing `DStream` for decompression.
    ///
    /// This is equivalent to calling:
    /// * `reset(SessionOnly)`
    /// * `disable_dictionary()`
    ///
    /// Wraps the `ZSTD_initCStream()` function.
    pub fn init(&mut self) -> SafeResult {
        self.2.clear();
        let code = unsafe { zstd_sys::ZSTD_initDStream(self.0.as_ptr()) };
        self.2.record(parse_code(code))
    }

    /// Wraps the `ZSTD_initDStream_usingDict()` function.
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    #[deprecated(note = "Use reset and load_dictionary instead.")]
    pub fn init_using_dict(&mut self, dict: &[u8]) -> SafeResult {
        self.2.clear();
        let code = unsafe {
            zstd_sys::ZSTD_initDStream_usingDict(
                self.0.as_ptr(),
                ptr_void(dict),
                dict.len(),
            )
        };
        parse_code(code)
    }

    /// Wraps the `ZSTD_initDStream_usingDDict()` function.
    ///
    /// Dictionary must outlive the context.
    ///
    /// ```compile_fail,E0515
    /// # #![allow(deprecated)]
    /// fn decompressor() -> zstd_safe::DCtx<'static> {
    ///     let ddict = zstd_safe::DDict::create(b"dictionary");
    ///     let mut dctx = zstd_safe::DCtx::create();
    ///     dctx.init_using_ddict(&ddict).unwrap();
    ///     dctx // `ddict` would be dropped while `dctx` still points at it.
    /// }
    /// ```
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    #[deprecated(note = "Use reset and ref_ddict instead.")]
    pub fn init_using_ddict<'b>(&mut self, ddict: &'a DDict<'b>) -> SafeResult
    where
        'b: 'a,
    {
        self.2.clear();
        // Safety: Just FFI
        let code = unsafe {
            zstd_sys::ZSTD_initDStream_usingDDict(
                self.0.as_ptr(),
                ddict.0.as_ptr(),
            )
        };
        self.2.record(parse_code(code))
    }

    /// Resets the state of the context.
    ///
    /// Depending on the reset mode, it can reset the session, the parameters, or both.
    ///
    /// Wraps the `ZSTD_DCtx_reset()` function.
    pub fn reset(&mut self, reset: ResetDirective) -> SafeResult {
        let res = parse_code(unsafe {
            zstd_sys::ZSTD_DCtx_reset(self.0.as_ptr(), reset.as_sys())
        });
        if res.is_ok() && reset.resets_session() {
            self.2.clear();
            self.3.streamed = false;
        }
        res
    }

    /// Loads a dictionary.
    ///
    /// This will let this context decompress frames that were compressed using this dictionary.
    ///
    /// The dictionary content will be copied internally and does not need to be kept alive after
    /// calling this function.
    ///
    /// If you need to use the same dictionary for multiple contexts, it may be more efficient to
    /// create a `DDict` first, then loads that.
    ///
    /// The dictionary will apply to all future frames, until a new dictionary is set.
    pub fn load_dictionary(&mut self, dict: &[u8]) -> SafeResult {
        parse_code(unsafe {
            zstd_sys::ZSTD_DCtx_loadDictionary(
                self.0.as_ptr(),
                ptr_void(dict),
                dict.len(),
            )
        })
    }

    /// Return to "no-dictionary" mode.
    ///
    /// This will disable any dictionary/prefix previously registered for future frames.
    pub fn disable_dictionary(&mut self) -> SafeResult {
        parse_code(unsafe {
            zstd_sys::ZSTD_DCtx_loadDictionary(
                self.0.as_ptr(),
                core::ptr::null(),
                0,
            )
        })
    }

    /// References a dictionary.
    ///
    /// This will let this context decompress frames compressed with the same dictionary.
    ///
    /// It will apply to all frames decompressed by this context (until a new dictionary is set).
    ///
    /// Wraps the `ZSTD_DCtx_refDDict()` function.
    ///
    /// Dictionary must outlive the context.
    pub fn ref_ddict<'b>(&mut self, ddict: &'a DDict<'b>) -> SafeResult
    where
        'b: 'a,
    {
        parse_code(unsafe {
            zstd_sys::ZSTD_DCtx_refDDict(self.0.as_ptr(), ddict.0.as_ptr())
        })
    }

    /// Use some prefix as single-use dictionary for the next frame.
    ///
    /// Just like a dictionary, this only works if compression was done with the same prefix.
    ///
    /// But unlike a dictionary, this only applies to the next frame.
    ///
    /// Wraps the `ZSTD_DCtx_refPrefix()` function.
    pub fn ref_prefix<'b>(&mut self, prefix: &'b [u8]) -> SafeResult
    where
        'b: 'a,
    {
        parse_code(unsafe {
            zstd_sys::ZSTD_DCtx_refPrefix(
                self.0.as_ptr(),
                ptr_void(prefix),
                prefix.len(),
            )
        })
    }

    /// Sets a decompression parameter.
    pub fn set_parameter(&mut self, param: DParameter) -> SafeResult {
        #[cfg(feature = "experimental")]
        use zstd_sys::ZSTD_dParameter::{
            ZSTD_d_experimentalParam1 as ZSTD_d_format,
            ZSTD_d_experimentalParam2 as ZSTD_d_stableOutBuffer,
            ZSTD_d_experimentalParam3 as ZSTD_d_forceIgnoreChecksum,
            ZSTD_d_experimentalParam4 as ZSTD_d_refMultipleDDicts,
        };

        use zstd_sys::ZSTD_dParameter::*;
        use DParameter::*;

        let (param, value) = match param {
            #[cfg(feature = "experimental")]
            Format(format) => (ZSTD_d_format, format as c_int),
            #[cfg(feature = "experimental")]
            StableOutBuffer(stable) => {
                (ZSTD_d_stableOutBuffer, stable as c_int)
            }
            #[cfg(feature = "experimental")]
            ForceIgnoreChecksum(force) => {
                (ZSTD_d_forceIgnoreChecksum, force as c_int)
            }
            #[cfg(feature = "experimental")]
            RefMultipleDDicts(value) => {
                (ZSTD_d_refMultipleDDicts, value as c_int)
            }

            WindowLogMax(value) => (ZSTD_d_windowLogMax, value as c_int),
        };

        parse_code(unsafe {
            zstd_sys::ZSTD_DCtx_setParameter(self.0.as_ptr(), param, value)
        })
    }

    /// Performs a step of a streaming decompression operation.
    ///
    /// This will read some data from `input` and/or write some data to `output`.
    ///
    /// # Returns
    ///
    /// * `Ok(0)` if the current frame just finished decompressing successfully.
    /// * `Ok(hint)` with a hint for the "ideal" amount of input data to provide in the next call.
    ///   Can be safely ignored.
    ///
    /// Wraps the `ZSTD_decompressStream()` function.
    pub fn decompress_stream<C: WriteBuf + ?Sized>(
        &mut self,
        output: &mut OutBuffer<'_, C>,
        input: &mut InBuffer<'_>,
    ) -> SafeResult {
        self.2.guard()?;
        self.3.streamed = true;
        let mut output = output.wrap();
        let mut input = input.wrap();
        let code = unsafe {
            zstd_sys::ZSTD_decompressStream(
                self.0.as_ptr(),
                ptr_mut(&mut output),
                ptr_mut(&mut input),
            )
        };
        let res = output.check(parse_code(code));
        self.2.record(input.check(res))
    }

    /// Wraps the `ZSTD_DStreamInSize()` function.
    ///
    /// Returns a hint for the recommended size of the input buffer for decompression.
    pub fn in_size() -> usize {
        unsafe { zstd_sys::ZSTD_DStreamInSize() }
    }

    /// Wraps the `ZSTD_DStreamOutSize()` function.
    ///
    /// Returns a hint for the recommended size of the output buffer for decompression.
    pub fn out_size() -> usize {
        unsafe { zstd_sys::ZSTD_DStreamOutSize() }
    }

    /// Wraps the `ZSTD_sizeof_DCtx()` function.
    pub fn sizeof(&self) -> usize {
        unsafe { zstd_sys::ZSTD_sizeof_DCtx(self.0.as_ptr()) }
    }

    /// Wraps the `ZSTD_decompressBlock()` function.
    ///
    /// # Safety
    ///
    /// The bytes written to `dst` become this context's history window, so `dst` must stay
    /// allocated and unmodified until the next call to `decompress_block` or `insert_block` on this
    /// context (or until this context is dropped), as the following block is decoded against it.
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    #[deprecated(
        note = "The block API is deprecated; use decompress_stream instead."
    )]
    pub unsafe fn decompress_block<C: WriteBuf + ?Sized>(
        &mut self,
        dst: &mut C,
        src: &[u8],
    ) -> SafeResult {
        self.3.streamed = true;
        unsafe {
            dst.write_from(|buffer, capacity| {
                parse_code(zstd_sys::ZSTD_decompressBlock(
                    self.0.as_ptr(),
                    buffer,
                    capacity,
                    ptr_void(src),
                    src.len(),
                ))
            })
        }
    }

    /// Wraps the `ZSTD_insertBlock()` function.
    ///
    /// # Safety
    ///
    /// `block` becomes this context's history window, so it must stay allocated and unmodified
    /// until the next call to `decompress_block` or `insert_block` on this context (or until this
    /// context is dropped), as the following block is decoded against it.
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    #[deprecated(
        note = "The block API is deprecated; use decompress_stream instead."
    )]
    pub unsafe fn insert_block(&mut self, block: &[u8]) -> usize {
        self.3.streamed = true;
        unsafe {
            zstd_sys::ZSTD_insertBlock(
                self.0.as_ptr(),
                ptr_void(block),
                block.len(),
            )
        }
    }
}

impl Drop for DCtx<'_> {
    fn drop(&mut self) {
        unsafe {
            zstd_sys::ZSTD_freeDCtx(self.0.as_ptr());
        }
    }
}

// Safety: as for `CCtx` - an owned heap allocation with no thread-local state.
unsafe impl Send for DCtx<'_> {}
// Non thread-safe methods already take `&mut self`, so it's fine to implement Sync here.
// Safety: as for `CCtx` - the mutating methods all take `&mut self`.
unsafe impl Sync for DCtx<'_> {}

/// Owns bytes that zstd points into, without copying them: a `Vec<u8>`, an
/// `Arc<[u8]>`, a memory map...
///
/// Kept as a raw pointer rather than a `Box`: the bytes may live inside the
/// owner, and moving a `Box` would assert unique access to memory zstd still
/// points into. The owner goes to its final place on the heap before its
/// bytes are taken, and is not touched again until it is dropped.
#[cfg(feature = "alloc")]
struct Owner(NonNull<dyn Send + Sync>);

#[cfg(feature = "alloc")]
impl Owner {
    /// Moves `owner` to the heap, and returns it with its bytes.
    ///
    /// The bytes stay valid until the `Owner` is dropped.
    fn new<P>(owner: P) -> (Self, *const u8, usize)
    where
        P: core::ops::Deref + Send + Sync + 'static,
        P::Target: AsRef<[u8]>,
    {
        let owner: *mut P =
            alloc::boxed::Box::into_raw(alloc::boxed::Box::new(owner));
        // Safety: `owner` is valid, and only ever read from now on.
        let bytes: &[u8] = unsafe { (**owner).as_ref() };
        let (ptr, len) = (bytes.as_ptr(), bytes.len());
        let owner: *mut (dyn Send + Sync) = owner;
        // Safety: from a `Box`, so not null.
        (Owner(unsafe { NonNull::new_unchecked(owner) }), ptr, len)
    }
}

#[cfg(feature = "alloc")]
impl Drop for Owner {
    fn drop(&mut self) {
        // Safety: made by `Box::into_raw`, and only read since.
        drop(unsafe { alloc::boxed::Box::from_raw(self.0.as_ptr()) });
    }
}

// Safety: the owner is `Send + Sync`; this only reads it, then drops it.
#[cfg(feature = "alloc")]
unsafe impl Send for Owner {}
#[cfg(feature = "alloc")]
unsafe impl Sync for Owner {}

/// Compression dictionary.
///
/// A digested dictionary, cheap to share: with the `alloc` feature (on targets
/// with pointer-sized atomics), it is a reference-counted handle, and cloning
/// it shares the same digested tables. A context referencing it
/// ([`CCtx::ref_cdict()`]) keeps its own handle, so the dictionary lives as
/// long as any context uses it, leaked ones included. Without `alloc`, it has
/// a single owner: move it into a context ([`CCtx::set_cdict()`]) or borrow
/// it.
///
/// The bytes it was made from are copied ([`CDict::create()`]), or, for a
/// dictionary made by reference, are `'static` or owned by the dictionary.
///
/// With `with-rust-allocator`, prepared dictionaries retain fixed compression
/// parameters rather than adapting them to later input sizes. This can change
/// compression ratio, speed, and memory usage compared with the C allocator.
pub struct CDict(CDictHandle);

#[cfg(all(feature = "alloc", target_has_atomic = "ptr"))]
type CDictHandle = alloc::sync::Arc<CDictInner>;
#[cfg(not(all(feature = "alloc", target_has_atomic = "ptr")))]
type CDictHandle = CDictInner;

/// What a [`CDict`] handle points to.
struct CDictInner {
    ptr: NonNull<zstd_sys::ZSTD_CDict>,
    /// Owns the bytes a dictionary made by reference points into, see
    /// `CDict::try_create_by_reference_owned()`. Dropped after the
    /// dictionary is freed.
    #[cfg(feature = "alloc")]
    #[allow(dead_code)] // Only kept alive.
    owner: Option<Owner>,
}

impl Drop for CDictInner {
    fn drop(&mut self) {
        // The dictionary first: it may point into the owner.
        // Safety: this is the last handle to it.
        unsafe {
            zstd_sys::ZSTD_freeCDict(self.ptr.as_ptr());
        }
        // Then `owner`, as a field.
    }
}

// Safety: a digested dictionary is immutable once built. zstd.h: "ZSTD_CDict
// can be created once and shared by multiple threads concurrently, since its
// usage is read-only". The owner is `Send + Sync`, and is only dropped.
unsafe impl Send for CDictInner {}
unsafe impl Sync for CDictInner {}

#[cfg(all(feature = "alloc", target_has_atomic = "ptr"))]
impl Clone for CDict {
    /// Returns another handle to the same digested dictionary.
    fn clone(&self) -> Self {
        CDict(self.0.clone())
    }
}

impl CDict {
    fn from_raw(ptr: *mut zstd_sys::ZSTD_CDict) -> Option<Self> {
        let inner = CDictInner {
            ptr: NonNull::new(ptr)?,
            #[cfg(feature = "alloc")]
            owner: None,
        };
        Some(CDict(inner.into()))
    }

    fn as_ptr(&self) -> *mut zstd_sys::ZSTD_CDict {
        self.0.ptr.as_ptr()
    }

    /// Prepare a dictionary to compress data.
    ///
    /// This will make it easier for compression contexts to load this dictionary.
    ///
    /// The dictionary content will be copied internally, and does not need to be kept around.
    ///
    /// # Panics
    ///
    /// If loading this dictionary failed.
    pub fn create(
        dict_buffer: &[u8],
        compression_level: CompressionLevel,
    ) -> Self {
        Self::try_create(dict_buffer, compression_level)
            .expect("zstd returned null pointer when creating dict")
    }

    /// Prepare a dictionary to compress data.
    ///
    /// This will make it easier for compression contexts to load this dictionary.
    ///
    /// The dictionary content will be copied internally, and does not need to be kept around.
    pub fn try_create(
        dict_buffer: &[u8],
        compression_level: CompressionLevel,
    ) -> Option<Self> {
        #[cfg(feature = "with-rust-allocator")]
        let ptr = unsafe {
            rust_allocator::create_cdict(
                dict_buffer,
                compression_level,
                zstd_sys::ZSTD_dictLoadMethod_e::ZSTD_dlm_byCopy,
            )
        };
        #[cfg(not(feature = "with-rust-allocator"))]
        let ptr = unsafe {
            zstd_sys::ZSTD_createCDict(
                ptr_void(dict_buffer),
                dict_buffer.len(),
                compression_level,
            )
        };
        Self::from_raw(ptr)
    }

    /// Digests `dict_buffer` by reference.
    ///
    /// # Safety
    ///
    /// `dict_buffer` must stay valid and unchanged until the dictionary is
    /// freed.
    #[cfg(feature = "experimental")]
    unsafe fn by_reference(
        dict_buffer: &[u8],
        compression_level: CompressionLevel,
    ) -> *mut zstd_sys::ZSTD_CDict {
        #[cfg(feature = "with-rust-allocator")]
        let ptr = rust_allocator::create_cdict(
            dict_buffer,
            compression_level,
            zstd_sys::ZSTD_dictLoadMethod_e::ZSTD_dlm_byRef,
        );
        #[cfg(not(feature = "with-rust-allocator"))]
        let ptr = zstd_sys::ZSTD_createCDict_byReference(
            ptr_void(dict_buffer),
            dict_buffer.len(),
            compression_level,
        );
        ptr
    }

    /// Prepare a dictionary to compress data, referencing `dict_buffer`.
    ///
    /// # Panics
    ///
    /// If loading this dictionary failed. See [`CDict::try_create_by_reference`].
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    pub fn create_by_reference(
        dict_buffer: &'static [u8],
        compression_level: CompressionLevel,
    ) -> Self {
        Self::try_create_by_reference(dict_buffer, compression_level)
            .expect("zstd returned null pointer")
    }

    /// Prepare a dictionary to compress data, referencing `dict_buffer`.
    ///
    /// The bytes are not copied: they are `'static` (`include_bytes!`, say).
    /// For bytes with an owner (a `Vec`, an `Arc<[u8]>`, a memory map), see
    /// [`CDict::try_create_by_reference_owned()`].
    ///
    /// Returns `None` if zstd could not load the dictionary, for instance
    /// because it starts with the dictionary magic number but its header is
    /// corrupt.
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    pub fn try_create_by_reference(
        dict_buffer: &'static [u8],
        compression_level: CompressionLevel,
    ) -> Option<Self> {
        // Safety: the bytes live forever.
        Self::from_raw(unsafe {
            Self::by_reference(dict_buffer, compression_level)
        })
    }

    /// Prepare a dictionary to compress data, referencing the bytes `owner`
    /// gives, which the dictionary then owns.
    ///
    /// # Panics
    ///
    /// If loading this dictionary failed. See
    /// [`CDict::try_create_by_reference_owned`].
    #[cfg(all(feature = "experimental", feature = "alloc"))]
    #[cfg_attr(
        feature = "doc-cfg",
        doc(cfg(all(feature = "experimental", feature = "alloc")))
    )]
    pub fn create_by_reference_owned<P>(
        owner: P,
        compression_level: CompressionLevel,
    ) -> Self
    where
        P: core::ops::Deref + Send + Sync + 'static,
        P::Target: AsRef<[u8]>,
    {
        Self::try_create_by_reference_owned(owner, compression_level)
            .expect("zstd returned null pointer")
    }

    /// Prepare a dictionary to compress data, referencing the bytes `owner`
    /// gives, which the dictionary then owns.
    ///
    /// The bytes are not copied: `owner` can be a `Vec<u8>`, an `Arc<[u8]>`,
    /// an `Arc<Vec<u8>>`, a memory map, a `bytes::Bytes`... It is dropped with
    /// the last handle to the dictionary.
    ///
    /// Returns `None` if zstd could not load the dictionary.
    #[cfg(all(feature = "experimental", feature = "alloc"))]
    #[cfg_attr(
        feature = "doc-cfg",
        doc(cfg(all(feature = "experimental", feature = "alloc")))
    )]
    pub fn try_create_by_reference_owned<P>(
        owner: P,
        compression_level: CompressionLevel,
    ) -> Option<Self>
    where
        P: core::ops::Deref + Send + Sync + 'static,
        P::Target: AsRef<[u8]>,
    {
        let (owner, ptr, len) = Owner::new(owner);
        // Safety: the dictionary owns the bytes from here on (below), and
        // frees itself before them.
        let ptr = unsafe {
            Self::by_reference(
                core::slice::from_raw_parts(ptr, len),
                compression_level,
            )
        };
        let inner = CDictInner {
            ptr: NonNull::new(ptr)?,
            owner: Some(owner),
        };
        Some(CDict(inner.into()))
    }

    /// Returns the _current_ memory usage of this dictionary.
    ///
    /// Note that this may change over time.
    pub fn sizeof(&self) -> usize {
        unsafe { zstd_sys::ZSTD_sizeof_CDict(self.as_ptr()) }
    }

    /// Returns the dictionary ID for this dict.
    ///
    /// Returns `None` if this dictionary is empty or invalid.
    pub fn get_dict_id(&self) -> Option<NonZeroU32> {
        NonZeroU32::new(unsafe {
            zstd_sys::ZSTD_getDictID_fromCDict(self.as_ptr()) as u32
        })
    }
}

/// Wraps the `ZSTD_createCDict()` function.
pub fn create_cdict(
    dict_buffer: &[u8],
    compression_level: CompressionLevel,
) -> CDict {
    CDict::create(dict_buffer, compression_level)
}

/// A digested decompression dictionary.
pub struct DDict<'a>(NonNull<zstd_sys::ZSTD_DDict>, PhantomData<&'a ()>);

impl DDict<'static> {
    /// Prepare a dictionary to decompress data.
    ///
    /// The dictionary content will be copied internally, and does not need to be kept around.
    ///
    /// # Panics
    ///
    /// If loading this dictionary failed. See [`DDict::try_create`].
    pub fn create(dict_buffer: &[u8]) -> Self {
        Self::try_create(dict_buffer)
            .expect("zstd returned null pointer when creating dict")
    }

    /// Prepare a dictionary to decompress data.
    ///
    /// The dictionary content will be copied internally, and does not need to be kept around.
    /// Returns `None` if zstd could not load the dictionary, for instance because it starts
    /// with the dictionary magic number but its header is corrupt.
    pub fn try_create(dict_buffer: &[u8]) -> Option<Self> {
        #[cfg(feature = "with-rust-allocator")]
        let ptr = unsafe {
            rust_allocator::create_ddict(
                dict_buffer,
                zstd_sys::ZSTD_dictLoadMethod_e::ZSTD_dlm_byCopy,
            )
        };
        #[cfg(not(feature = "with-rust-allocator"))]
        let ptr = unsafe {
            zstd_sys::ZSTD_createDDict(
                ptr_void(dict_buffer),
                dict_buffer.len(),
            )
        };
        Some(DDict(NonNull::new(ptr)?, PhantomData))
    }
}

impl<'a> DDict<'a> {
    pub fn sizeof(&self) -> usize {
        unsafe { zstd_sys::ZSTD_sizeof_DDict(self.0.as_ptr()) }
    }

    /// Wraps the `ZSTD_createDDict_byReference()` function.
    ///
    /// The dictionary will keep referencing `dict_buffer`.
    ///
    /// # Panics
    ///
    /// If loading this dictionary failed. See [`DDict::try_create_by_reference`].
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    pub fn create_by_reference(dict_buffer: &'a [u8]) -> Self {
        Self::try_create_by_reference(dict_buffer)
            .expect("zstd returned null pointer")
    }

    /// Wraps the `ZSTD_createDDict_byReference()` function.
    ///
    /// The dictionary will keep referencing `dict_buffer`. Returns `None` if
    /// zstd could not load the dictionary, for instance because it starts with
    /// the dictionary magic number but its header is corrupt.
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    pub fn try_create_by_reference(dict_buffer: &'a [u8]) -> Option<Self> {
        #[cfg(feature = "with-rust-allocator")]
        let ptr = unsafe {
            rust_allocator::create_ddict(
                dict_buffer,
                zstd_sys::ZSTD_dictLoadMethod_e::ZSTD_dlm_byRef,
            )
        };
        #[cfg(not(feature = "with-rust-allocator"))]
        let ptr = unsafe {
            zstd_sys::ZSTD_createDDict_byReference(
                ptr_void(dict_buffer),
                dict_buffer.len(),
            )
        };
        Some(DDict(NonNull::new(ptr)?, PhantomData))
    }

    /// Returns the dictionary ID for this dict.
    ///
    /// Returns `None` if this dictionary is empty or invalid.
    pub fn get_dict_id(&self) -> Option<NonZeroU32> {
        NonZeroU32::new(unsafe {
            zstd_sys::ZSTD_getDictID_fromDDict(self.0.as_ptr()) as u32
        })
    }
}

/// Wraps the `ZSTD_createDDict()` function.
///
/// It copies the dictionary internally, so the resulting `DDict` is `'static`.
pub fn create_ddict(dict_buffer: &[u8]) -> DDict<'static> {
    DDict::create(dict_buffer)
}

impl<'a> Drop for DDict<'a> {
    fn drop(&mut self) {
        unsafe {
            zstd_sys::ZSTD_freeDDict(self.0.as_ptr());
        }
    }
}

// Safety: like `CDict`, a digested dictionary is only read once built - the
// contexts referencing it never write to it.
unsafe impl<'a> Send for DDict<'a> {}
unsafe impl<'a> Sync for DDict<'a> {}

/// A shared thread pool for one or more compression contexts
#[cfg(all(feature = "experimental", feature = "zstdmt"))]
#[cfg_attr(
    feature = "doc-cfg",
    doc(cfg(all(feature = "experimental", feature = "zstdmt")))
)]
pub struct ThreadPool(NonNull<zstd_sys::ZSTD_threadPool>);

#[cfg(all(feature = "experimental", feature = "zstdmt"))]
#[cfg_attr(
    feature = "doc-cfg",
    doc(cfg(all(feature = "experimental", feature = "zstdmt")))
)]
impl ThreadPool {
    /// Create a thread pool with the specified number of threads.
    ///
    /// # Panics
    ///
    /// If creating the thread pool failed.
    pub fn new(num_threads: usize) -> Self {
        Self::try_new(num_threads)
            .expect("zstd returned null pointer when creating thread pool")
    }

    /// Create a thread pool with the specified number of threads.
    pub fn try_new(num_threads: usize) -> Option<Self> {
        Some(Self(NonNull::new(unsafe {
            zstd_sys::ZSTD_createThreadPool(num_threads)
        })?))
    }
}

#[cfg(all(feature = "experimental", feature = "zstdmt"))]
#[cfg_attr(
    feature = "doc-cfg",
    doc(cfg(all(feature = "experimental", feature = "zstdmt")))
)]
impl Drop for ThreadPool {
    fn drop(&mut self) {
        unsafe {
            zstd_sys::ZSTD_freeThreadPool(self.0.as_ptr());
        }
    }
}

#[cfg(all(feature = "experimental", feature = "zstdmt"))]
#[cfg_attr(
    feature = "doc-cfg",
    doc(cfg(all(feature = "experimental", feature = "zstdmt")))
)]
// Safety: the pool owns its worker threads and the queue guarding them; none
// of that is tied to the thread that created it.
unsafe impl Send for ThreadPool {}
#[cfg(all(feature = "experimental", feature = "zstdmt"))]
#[cfg_attr(
    feature = "doc-cfg",
    doc(cfg(all(feature = "experimental", feature = "zstdmt")))
)]
// Safety: sharing is what the pool is for - zstd.h offers these functions to
// "share a thread pool among multiple compression contexts" - and its work
// queue is guarded by an internal mutex.
unsafe impl Sync for ThreadPool {}

/// Wraps the `ZSTD_decompress_usingDDict()` function.
///
/// Only as many bytes as returned at the start of `dst` are meaningful: zstd
/// may overwrite the rest.
pub fn decompress_using_ddict(
    dctx: &mut DCtx<'_>,
    dst: &mut [u8],
    src: &[u8],
    ddict: &DDict<'_>,
) -> SafeResult {
    dctx.decompress_using_ddict(dst, src, ddict)
}

/// Compression stream.
///
/// Same as `CCtx`.
pub type CStream<'a> = CCtx<'a>;

// `CStream` is an alias for `CCtx`, and shares its `Send` and `Sync` impls.

/// Allocates a new `CStream`.
pub fn create_cstream<'a>() -> CStream<'a> {
    CCtx::create()
}

/// Prepares an existing `CStream` for compression at the given level.
pub fn init_cstream(
    zcs: &mut CStream<'_>,
    compression_level: CompressionLevel,
) -> SafeResult {
    zcs.init(compression_level)
}

#[derive(Debug)]
/// Wrapper around an input buffer.
///
/// Bytes will be read starting at `src[pos]`.
///
/// `pos` will be updated after reading.
///
/// # Invariant
///
/// `pos <= src.len()`
///
/// Both fields are public, so this is checked (and panics if it does not hold)
/// whenever the buffer is given to zstd.
pub struct InBuffer<'a> {
    pub src: &'a [u8],
    pub pos: usize,
}

/// Describe a bytes container, like `Vec<u8>`.
///
/// Represents a contiguous segment of allocated memory, a prefix of which is initialized.
///
/// It allows starting from an uninitializes chunk of memory and writing to it, progressively
/// initializing it. No re-allocation typically occur after the initial creation.
///
/// The main implementors are:
/// * `Vec<u8>` and similar structures. These hold both a length (initialized data) and a capacity
///   (allocated memory).
///
///   Use `Vec::with_capacity` to create an empty `Vec` with non-zero capacity, and the length
///   field will be updated to cover the data written to it (as long as it fits in the given
///   capacity).
/// * `[u8]` and `[u8; N]`. These must start already-initialized, and will not be resized. It will
///   be up to the caller to only use the part that was written (as returned by the various writing
///   operations).
/// * `std::io::Cursor<T: WriteBuf>`. This will ignore data before the cursor's position, and
///   append data after that.
///
/// # Safety
///
/// An implementation has to keep three promises, because the zstd functions
/// this is handed to write straight through the pointer:
///
/// * `as_mut_ptr()` points at a buffer that is valid to write `capacity()`
///   bytes to.
/// * `as_slice()` covers only initialized bytes - never any of the capacity
///   that has not been written yet.
/// * `filled_until(n)` is what marks the first `n` bytes as initialized, so
///   after it returns, `as_slice()` must cover them.
pub unsafe trait WriteBuf {
    /// Returns the valid data part of this container. Should only cover initialized data.
    fn as_slice(&self) -> &[u8];

    /// Returns the full capacity of this container. May include uninitialized data.
    fn capacity(&self) -> usize;

    /// Returns a pointer to the start of the data.
    fn as_mut_ptr(&mut self) -> *mut u8;

    /// Indicates that the first `n` bytes of the container have been written.
    ///
    /// # Safety
    ///
    /// The first `n` bytes of this buffer must actually have been initialized.
    unsafe fn filled_until(&mut self, n: usize);

    /// Call the given closure using the pointer and capacity from `self`.
    ///
    /// The closure returns a parseable code which, if valid, is how many bytes
    /// it wrote to `self`.
    ///
    /// # Safety
    ///
    /// The closure must treat its first argument as pointing to potentially
    /// uninitialized memory, and must not read from it. It must have written
    /// at least `n` bytes contiguously from that pointer, where `n` is the
    /// value it returns.
    unsafe fn write_from<F>(&mut self, f: F) -> SafeResult
    where
        F: FnOnce(*mut c_void, usize) -> SafeResult,
    {
        let capacity = self.capacity();
        let res = f(ptr_mut_void(self), capacity);
        if let Ok(n) = res {
            // Nothing can have been written past the end of the buffer; a
            // larger count would mark memory we do not own as initialized.
            assert!(n <= capacity, "zstd reported writing past the buffer");
            self.filled_until(n);
        }
        res
    }
}

/// The position of a `Cursor`, as an index into the buffer it wraps.
///
/// `Cursor` stores the position as a `u64` and lets it be set anywhere, so on a
/// target where `usize` is narrower it can name an offset no buffer can hold.
/// Saturating leaves such a position out of range, where the bounds checks
/// below reject it; casting would wrap it around into a valid-looking offset
/// and quietly read or write the wrong part of the buffer.
#[cfg(feature = "std")]
fn cursor_position<T>(cursor: &std::io::Cursor<T>) -> usize {
    use core::convert::TryFrom;

    usize::try_from(cursor.position()).unwrap_or(usize::MAX)
}

#[cfg(feature = "std")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "std")))]
unsafe impl<T> WriteBuf for std::io::Cursor<T>
where
    T: WriteBuf,
{
    fn as_slice(&self) -> &[u8] {
        // A cursor past the inner buffer's initialized part has nothing
        // initialized after it yet (see `filled_until` below).
        self.get_ref()
            .as_slice()
            .get(cursor_position(self)..)
            .unwrap_or(&[])
    }

    fn capacity(&self) -> usize {
        self.get_ref()
            .capacity()
            .saturating_sub(cursor_position(self))
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        let start = cursor_position(self);
        assert!(start <= self.get_ref().capacity());
        // Safety: start is still in the same memory allocation
        unsafe { self.get_mut().as_mut_ptr().add(start) }
    }

    unsafe fn filled_until(&mut self, n: usize) {
        // Here we assume data _before_ self.position() was already initialized.
        // Egh it's not actually guaranteed by Cursor? So let's guarantee it ourselves.
        // Since the cursor wraps another `WriteBuf`, we know how much data is initialized there.
        let position = cursor_position(self);
        let capacity = self.get_ref().capacity();
        if n == 0 && position > capacity {
            // Nothing was written, and nothing could have been: there is
            // no room at all after the cursor.
            return;
        }
        // Otherwise the caller wrote `n` bytes starting at `position`, so
        // `position` is inside the buffer. Checking it before the zero-fill
        // below keeps a position that could not be converted from running off
        // the end.
        //
        // Note that `n == 0` still truncates the data after the cursor, like
        // any other `n` does: an empty result must not leave a previous one
        // in place.
        assert!(position <= capacity);
        let initialized = self.get_ref().as_slice().len();
        if let Some(uninitialized) = position.checked_sub(initialized) {
            // Here, the cursor is further than the known-initialized part.
            // Cursor's solution is to pad with zeroes, so let's do the same.
            // We'll zero bytes from the end of valid data (as_slice().len()) to the cursor position.

            // Safety:
            // * We checked `position <= self.get_ref().capacity()`
            // * This means that `position` is within (or one past) the nested
            //   pointer's allocation.
            // * Finally, `initialized + uninitialized = position`, so the entire byte
            //   range here is within the allocation
            unsafe {
                self.get_mut()
                    .as_mut_ptr()
                    .add(initialized)
                    .write_bytes(0u8, uninitialized)
            };
        }

        let start = position;
        assert!(start + n <= self.get_ref().capacity());
        self.get_mut().filled_until(start + n);
    }
}

#[cfg(feature = "alloc")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "alloc")))]
unsafe impl WriteBuf for &mut alloc::vec::Vec<u8> {
    fn as_slice(&self) -> &[u8] {
        alloc::vec::Vec::as_slice(self)
    }

    fn capacity(&self) -> usize {
        alloc::vec::Vec::capacity(self)
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        alloc::vec::Vec::as_mut_ptr(self)
    }

    unsafe fn filled_until(&mut self, n: usize) {
        alloc::vec::Vec::set_len(self, n)
    }
}

#[cfg(feature = "alloc")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "alloc")))]
unsafe impl WriteBuf for alloc::vec::Vec<u8> {
    fn as_slice(&self) -> &[u8] {
        &self[..]
    }
    fn capacity(&self) -> usize {
        self.capacity()
    }
    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.as_mut_ptr()
    }
    unsafe fn filled_until(&mut self, n: usize) {
        self.set_len(n);
    }
}

#[cfg(feature = "arrays")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "arrays")))]
unsafe impl<const N: usize> WriteBuf for [u8; N] {
    fn as_slice(&self) -> &[u8] {
        self
    }
    fn capacity(&self) -> usize {
        self.len()
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self[..].as_mut_ptr()
    }

    unsafe fn filled_until(&mut self, _n: usize) {
        // Assume the slice is already initialized
    }
}

unsafe impl WriteBuf for [u8] {
    fn as_slice(&self) -> &[u8] {
        self
    }
    fn capacity(&self) -> usize {
        self.len()
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.as_mut_ptr()
    }

    unsafe fn filled_until(&mut self, _n: usize) {
        // Assume the slice is already initialized
    }
}

/*
// This is possible, but... why?
unsafe impl<'a> WriteBuf for OutBuffer<'a, [u8]> {
    fn as_slice(&self) -> &[u8] {
        self.dst
    }
    fn capacity(&self) -> usize {
        self.dst.len()
    }
    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.dst.as_mut_ptr()
    }
    unsafe fn filled_until(&mut self, n: usize) {
        self.pos = n;
    }
}
*/

#[derive(Debug)]
/// Wrapper around an output buffer.
///
/// `C` is usually either `[u8]` or `Vec<u8>`.
///
/// Bytes will be written starting at `dst[pos]`.
///
/// `pos` will be updated after writing.
///
/// # Invariant
///
/// `pos <= dst.capacity()`
pub struct OutBuffer<'a, C: WriteBuf + ?Sized> {
    dst: &'a mut C,
    pos: usize,
}

/// Convenience method to get a mut pointer from a mut ref.
fn ptr_mut<B>(ptr_void: &mut B) -> *mut B {
    ptr_void as *mut B
}

/// Interface between a C-level ZSTD_outBuffer and a rust-level `OutBuffer`.
///
/// Will update the parent buffer from the C buffer on drop.
struct OutBufferWrapper<'a, 'b, C: WriteBuf + ?Sized> {
    buf: zstd_sys::ZSTD_outBuffer,
    parent: &'a mut OutBuffer<'b, C>,
}

impl<'a, 'b: 'a, C: WriteBuf + ?Sized> Deref for OutBufferWrapper<'a, 'b, C> {
    type Target = zstd_sys::ZSTD_outBuffer;

    fn deref(&self) -> &Self::Target {
        &self.buf
    }
}

impl<'a, 'b: 'a, C: WriteBuf + ?Sized> DerefMut
    for OutBufferWrapper<'a, 'b, C>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.buf
    }
}

impl<'a, C: WriteBuf + ?Sized> OutBuffer<'a, C> {
    /// Returns a new `OutBuffer` around the given slice.
    ///
    /// Starts with `pos = 0`.
    pub fn around(dst: &'a mut C) -> Self {
        OutBuffer { dst, pos: 0 }
    }

    /// Returns a new `OutBuffer` around the given slice, starting at the given position.
    ///
    /// If `pos` is past the initialized part of `dst` (for example a `Vec`
    /// whose `len()` is smaller than `pos`), the gap is zero-filled, as a
    /// `Cursor` would do.
    ///
    /// # Panics
    ///
    /// If `pos > dst.capacity()`.
    pub fn around_pos(dst: &'a mut C, pos: usize) -> Self {
        if pos > dst.capacity() {
            panic!("Given position outside of the buffer bounds.");
        }

        let initialized = dst.as_slice().len();
        if pos > initialized {
            // Writing to this buffer marks everything before `pos` as
            // initialized (see `set_pos`), so it has to be before we start.
            // Safety: `initialized..pos` lies within `capacity()`, which
            // `WriteBuf` promises is valid to write to.
            unsafe {
                dst.as_mut_ptr()
                    .add(initialized)
                    .write_bytes(0u8, pos - initialized);
                dst.filled_until(pos);
            }
        }

        OutBuffer { dst, pos }
    }

    /// Returns the current cursor position.
    ///
    /// Guaranteed to be <= self.capacity()
    pub fn pos(&self) -> usize {
        assert!(self.pos <= self.dst.capacity());
        self.pos
    }

    /// Returns the capacity of the underlying buffer.
    pub fn capacity(&self) -> usize {
        self.dst.capacity()
    }

    /// Sets the new cursor position.
    ///
    /// # Panics
    ///
    /// If `pos > self.dst.capacity()`.
    ///
    /// # Safety
    ///
    /// Data up to `pos` must have actually been written to.
    pub unsafe fn set_pos(&mut self, pos: usize) {
        if pos > self.dst.capacity() {
            panic!("Given position outside of the buffer bounds.");
        }

        self.dst.filled_until(pos);

        self.pos = pos;
    }

    fn wrap<'b>(&'b mut self) -> OutBufferWrapper<'b, 'a, C> {
        OutBufferWrapper {
            buf: zstd_sys::ZSTD_outBuffer {
                dst: ptr_mut_void(self.dst),
                size: self.dst.capacity(),
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
        &self.dst.as_slice()[..pos]
    }

    /// Returns a pointer to the start of this buffer.
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.dst.as_mut_ptr()
    }
}

impl<'a, 'b, C: WriteBuf + ?Sized> OutBufferWrapper<'a, 'b, C> {
    /// Passes `res` through, and forgets the position reported by zstd if it
    /// is an error, so the parent keeps the one from before the call.
    ///
    /// zstd.h makes no promise about `pos` after a failed call, and the legacy
    /// decoders report the whole output buffer as written when they fail; only
    /// a successful call vouches for the bytes up to `pos` being initialized.
    fn check(&mut self, res: SafeResult) -> SafeResult {
        if res.is_err() {
            self.buf.pos = self.parent.pos;
        }
        res
    }
}

impl<'a, 'b, C: WriteBuf + ?Sized> Drop for OutBufferWrapper<'a, 'b, C> {
    fn drop(&mut self) {
        // Safe because `self.buf.pos` either comes from a successful call,
        // which guarantees that data until there has been written, or was put
        // back to the parent's position by `check()`.
        unsafe { self.parent.set_pos(self.buf.pos) };
    }
}

struct InBufferWrapper<'a, 'b> {
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

    /// Returns the current cursor position.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Sets the new cursor position.
    ///
    /// # Panics
    ///
    /// If `pos > self.src.len()`.
    pub fn set_pos(&mut self, pos: usize) {
        if pos > self.src.len() {
            panic!("Given position outside of the buffer bounds.");
        }
        self.pos = pos;
    }

    fn wrap<'b>(&'b mut self) -> InBufferWrapper<'b, 'a> {
        // The fields are public, so `pos` may not have gone through `set_pos`.
        // Not all zstd functions check it before reading from `src + pos`.
        if self.pos > self.src.len() {
            panic!("Given position outside of the buffer bounds.");
        }

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

impl<'a, 'b> InBufferWrapper<'a, 'b> {
    /// Passes `res` through, and forgets the position reported by zstd if it
    /// is an error, so the parent keeps the one from before the call.
    ///
    /// See `OutBufferWrapper::check()`: a failed call consumed nothing usable,
    /// and the legacy decoders report the whole input as read.
    fn check(&mut self, res: SafeResult) -> SafeResult {
        if res.is_err() {
            self.buf.pos = self.parent.pos;
        }
        res
    }
}

impl<'a, 'b> Drop for InBufferWrapper<'a, 'b> {
    fn drop(&mut self) {
        self.parent.set_pos(self.buf.pos);
    }
}

/// A Decompression stream.
///
/// Same as `DCtx`.
pub type DStream<'a> = DCtx<'a>;

// Some functions work on a "frame prefix".
// TODO: Define `struct FramePrefix(&[u8]);` and move these functions to it?
//
// Some other functions work on a dictionary (not CDict or DDict).
// Same thing?

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
/// Args:
/// * `src`: A prefix of the compressed frame. It should at least include the frame header.
///
/// Returns:
/// * `Err(ContentSizeError)` if `src` is too small of a prefix, or if it appears corrupted.
/// * `Ok(None)` if the frame does not include a content size.
/// * `Ok(Some(content_size_in_bytes))` otherwise.
pub fn get_frame_content_size(
    src: &[u8],
) -> Result<Option<u64>, ContentSizeError> {
    parse_content_size(unsafe {
        zstd_sys::ZSTD_getFrameContentSize(ptr_void(src), src.len())
    })
}

/// Wraps the `ZSTD_findDecompressedSize()` function.
///
/// `src` should be exactly a sequence of ZSTD frames.
#[cfg(feature = "experimental")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
pub fn find_decompressed_size(
    src: &[u8],
) -> Result<Option<u64>, ContentSizeError> {
    parse_content_size(unsafe {
        zstd_sys::ZSTD_findDecompressedSize(ptr_void(src), src.len())
    })
}

/// Wraps the `ZSTD_isFrame()` function.
#[cfg(feature = "experimental")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
pub fn is_frame(buffer: &[u8]) -> bool {
    unsafe { zstd_sys::ZSTD_isFrame(ptr_void(buffer), buffer.len()) > 0 }
}

/// Wraps the `ZSTD_getDictID_fromDict()` function.
///
/// Returns `None` if the dictionary is not a valid zstd dictionary.
pub fn get_dict_id_from_dict(dict: &[u8]) -> Option<NonZeroU32> {
    NonZeroU32::new(unsafe {
        zstd_sys::ZSTD_getDictID_fromDict(ptr_void(dict), dict.len()) as u32
    })
}

/// Wraps the `ZSTD_getDictID_fromFrame()` function.
///
/// Returns `None` if the dictionary ID could not be decoded. This may happen if:
/// * The frame was not encoded with a dictionary.
/// * The frame intentionally did not include dictionary ID.
/// * The dictionary was non-conformant.
/// * `src` is too small and does not include the frame header.
/// * `src` is not a valid zstd frame prefix.
pub fn get_dict_id_from_frame(src: &[u8]) -> Option<NonZeroU32> {
    NonZeroU32::new(unsafe {
        zstd_sys::ZSTD_getDictID_fromFrame(ptr_void(src), src.len()) as u32
    })
}

/// What kind of context reset should be applied.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ResetDirective {
    /// Only the session will be reset.
    ///
    /// All parameters will be preserved (including the dictionary).
    /// But any frame being processed will be dropped.
    ///
    /// It can be useful to start re-using a context after an error or when an
    /// ongoing compression is no longer needed.
    SessionOnly,

    /// Only reset parameters (including dictionary or referenced prefix).
    ///
    /// All parameters will be reset to default values.
    ///
    /// This can only be done between sessions - no compression or decompression must be ongoing.
    Parameters,

    /// Reset both the session and parameters.
    ///
    /// The result is similar to a newly created context.
    SessionAndParameters,
}

impl ResetDirective {
    /// Does this drop a session in progress?
    ///
    /// Only a session reset brings a context back from the undefined state an
    /// error can leave it in - `Parameters` alone is refused while a session is
    /// open.
    fn resets_session(self) -> bool {
        matches!(
            self,
            ResetDirective::SessionOnly | ResetDirective::SessionAndParameters
        )
    }

    fn as_sys(self) -> zstd_sys::ZSTD_ResetDirective {
        match self {
            ResetDirective::SessionOnly => zstd_sys::ZSTD_ResetDirective::ZSTD_reset_session_only,
            ResetDirective::Parameters => zstd_sys::ZSTD_ResetDirective::ZSTD_reset_parameters,
            ResetDirective::SessionAndParameters => zstd_sys::ZSTD_ResetDirective::ZSTD_reset_session_and_parameters,
        }
    }
}

#[cfg(feature = "experimental")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum FrameFormat {
    /// Regular zstd format.
    One = zstd_sys::ZSTD_format_e::ZSTD_f_zstd1 as u32,

    /// Skip the 4 bytes identifying the content as zstd-compressed data.
    Magicless = zstd_sys::ZSTD_format_e::ZSTD_f_zstd1_magicless as u32,
}

#[cfg(feature = "experimental")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum DictAttachPref {
    DefaultAttach =
        zstd_sys::ZSTD_dictAttachPref_e::ZSTD_dictDefaultAttach as u32,
    ForceAttach = zstd_sys::ZSTD_dictAttachPref_e::ZSTD_dictForceAttach as u32,
    ForceCopy = zstd_sys::ZSTD_dictAttachPref_e::ZSTD_dictForceCopy as u32,
    ForceLoad = zstd_sys::ZSTD_dictAttachPref_e::ZSTD_dictForceLoad as u32,
}

#[cfg(feature = "experimental")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ParamSwitch {
    Auto = zstd_sys::ZSTD_ParamSwitch_e::ZSTD_ps_auto as u32,
    Enable = zstd_sys::ZSTD_ParamSwitch_e::ZSTD_ps_enable as u32,
    Disable = zstd_sys::ZSTD_ParamSwitch_e::ZSTD_ps_disable as u32,
}

/// A compression parameter.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CParameter {
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    RSyncable(bool),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    Format(FrameFormat),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    ForceMaxWindow(bool),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    ForceAttachDict(DictAttachPref),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    LiteralCompressionMode(ParamSwitch),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    SrcSizeHint(u32),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    EnableDedicatedDictSearch(bool),

    /// Whether zstd may keep referencing the input buffer instead of copying
    /// it.
    ///
    /// This requires the input to stay allocated and unmodified until the
    /// frame is complete, which a borrowed `InBuffer` cannot guarantee:
    /// `CCtx::set_parameter` refuses to enable it and returns an error.
    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    StableInBuffer(bool),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    StableOutBuffer(bool),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    BlockDelimiters(bool),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    ValidateSequences(bool),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    UseBlockSplitter(ParamSwitch),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    UseRowMatchFinder(ParamSwitch),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    DeterministicRefPrefix(bool),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    PrefetchCDictTables(ParamSwitch),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    EnableSeqProducerFallback(bool),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    MaxBlockSize(u32),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    SearchForExternalRepcodes(ParamSwitch),

    /// Target CBlock size.
    ///
    /// Tries to make compressed blocks fit in this size (not a guarantee, just a target).
    /// Useful to reduce end-to-end latency in low-bandwidth environments.
    ///
    /// No target when the value is 0.
    TargetCBlockSize(u32),

    /// Compression level to use.
    ///
    /// Compression levels are global presets for the other compression parameters.
    CompressionLevel(CompressionLevel),

    /// Maximum allowed back-reference distance.
    ///
    /// The actual distance is 2 power "this value".
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

    /// How many threads will be spawned.
    ///
    /// With a default value of `0`, `compress_stream*` functions block until they complete.
    ///
    /// With any other value (including 1, a single compressing thread), these methods directly
    /// return, and the actual compression is done in the background (until a flush is requested).
    /// A context compressing that way from a borrowed dictionary or prefix
    /// must not be leaked, see [`CCtx`](CCtx#worker-threads-and-leaks).
    ///
    /// Note: this will only work if the `zstdmt` feature is activated.
    NbWorkers(u32),

    /// Size in bytes of a compression job.
    ///
    /// Does not have any effect when `NbWorkers` is set to 0.
    ///
    /// The default value of 0 finds the best job size based on the compression parameters.
    ///
    /// Note: this will only work if the `zstdmt` feature is activated.
    JobSize(u32),

    /// Specifies how much overlap must be given to each worker.
    ///
    /// Possible values:
    ///
    /// * `0` (default value): automatic overlap based on compression strategy.
    /// * `1`: No overlap
    /// * `1 < n < 9`: Overlap a fraction of the window size, defined as `1/(2 ^ 9-n)`.
    /// * `9`: Full overlap (as long as the window)
    /// * `9 < m`: Will return an error.
    ///
    /// Note: this will only work if the `zstdmt` feature is activated.
    OverlapSizeLog(u32),
}

/// A decompression parameter.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DParameter {
    WindowLogMax(u32),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    /// See `FrameFormat`.
    Format(FrameFormat),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    StableOutBuffer(bool),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    ForceIgnoreChecksum(bool),

    #[cfg(feature = "experimental")]
    #[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
    RefMultipleDDicts(bool),
}

/// Wraps the `ZDICT_trainFromBuffer()` function.
#[cfg(feature = "zdict_builder")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "zdict_builder")))]
pub fn train_from_buffer<C: WriteBuf + ?Sized>(
    dict_buffer: &mut C,
    samples_buffer: &[u8],
    samples_sizes: &[usize],
) -> SafeResult {
    // A plain `sum()` wraps around when overflow checks are disabled, and
    // would accept sizes reaching far outside of `samples_buffer`.
    let total_size = samples_sizes
        .iter()
        .try_fold(0usize, |total, &size| total.checked_add(size));
    assert_eq!(
        Some(samples_buffer.len()),
        total_size,
        "sample sizes don't add up to the samples buffer length"
    );

    unsafe {
        dict_buffer.write_from(|buffer, capacity| {
            parse_code(zstd_sys::ZDICT_trainFromBuffer(
                buffer,
                capacity,
                ptr_void(samples_buffer),
                samples_sizes.as_ptr(),
                samples_sizes.len() as u32,
            ))
        })
    }
}

/// Wraps the `ZDICT_getDictID()` function.
#[cfg(feature = "zdict_builder")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "zdict_builder")))]
pub fn get_dict_id(dict_buffer: &[u8]) -> Option<NonZeroU32> {
    NonZeroU32::new(unsafe {
        zstd_sys::ZDICT_getDictID(ptr_void(dict_buffer), dict_buffer.len())
    })
}

/// Wraps the `ZSTD_getBlockSize()` function.
#[cfg(feature = "experimental")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
#[deprecated(
    note = "The block API is deprecated; use the normal compression API instead."
)]
pub fn get_block_size(cctx: &CCtx) -> usize {
    unsafe { zstd_sys::ZSTD_getBlockSize(cctx.0.as_ptr()) }
}

/// Wraps the `ZSTD_decompressBound` function
#[cfg(feature = "experimental")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
pub fn decompress_bound(data: &[u8]) -> Result<u64, ErrorCode> {
    let bound =
        unsafe { zstd_sys::ZSTD_decompressBound(ptr_void(data), data.len()) };
    if is_error(bound as usize) {
        Err(bound as usize)
    } else {
        Ok(bound)
    }
}

/// Given a buffer of size `src_size`, returns the maximum number of sequences that can ge
/// generated.
#[cfg(feature = "experimental")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
pub fn sequence_bound(src_size: usize) -> usize {
    // Safety: Just FFI.
    unsafe { zstd_sys::ZSTD_sequenceBound(src_size) }
}

/// Returns the minimum extra space when output and input buffer overlap.
///
/// When using in-place decompression, the output buffer must be at least this much bigger (in
/// bytes) than the input buffer. The extra space must be at the front of the output buffer (the
/// input buffer must be at the end of the output buffer).
#[cfg(feature = "experimental")]
#[cfg_attr(feature = "doc-cfg", doc(cfg(feature = "experimental")))]
pub fn decompression_margin(
    compressed_data: &[u8],
) -> Result<usize, ErrorCode> {
    parse_code(unsafe {
        zstd_sys::ZSTD_decompressionMargin(
            ptr_void(compressed_data),
            compressed_data.len(),
        )
    })
}
