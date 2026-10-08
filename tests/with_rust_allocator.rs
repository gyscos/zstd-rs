//! With the `with-rust-allocator` feature, every encoder/decoder this crate
//! builds - including ones created by third-party code that never heard of
//! `try_create_with_global_allocator` - must allocate through Rust's global
//! allocator.
#![cfg(feature = "with-rust-allocator")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

/// Bytes currently held through the global allocator.
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
/// Bytes ever requested from the global allocator (monotonic).
static TOTAL_BYTES: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE_BYTES.fetch_add(layout.size(), Relaxed);
        TOTAL_BYTES.fetch_add(layout.size(), Relaxed);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Relaxed);
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn input() -> Vec<u8> {
    b"the quick brown fox jumps over the lazy dog; ".repeat(10_000)
}

/// The compression workspace at the default level is around 1 MiB; anything
/// above this clearly came from zstd rather than from our own buffers.
const WORKSPACE_FLOOR: usize = 512 * 1024;

/// Bulk API, exactly as arrow-rs uses it: a `Compressor` kept around and
/// reused, and `decompress_to_buffer` on a `Decompressor`.
fn bulk_api_uses_global_allocator() {
    let input = input();
    let mut compressed = Vec::with_capacity(input.len());
    let mut decompressed = vec![0u8; input.len()];

    let before = LIVE_BYTES.load(Relaxed);
    {
        let mut compressor =
            zstd::bulk::Compressor::new(3).expect("compressor");
        compressor
            .compress_to_buffer(&input, &mut compressed)
            .expect("compression failed");
        assert!(
            LIVE_BYTES.load(Relaxed) - before > WORKSPACE_FLOOR,
            "bulk::Compressor workspace is not visible to the global allocator"
        );
    }
    assert_eq!(LIVE_BYTES.load(Relaxed), before, "bulk::Compressor leaked");

    let before = LIVE_BYTES.load(Relaxed);
    {
        let mut decompressor =
            zstd::bulk::Decompressor::new().expect("decompressor");
        let n = decompressor
            .decompress_to_buffer(&compressed, &mut decompressed[..])
            .expect("decompression failed");
        assert_eq!(n, input.len());
        assert!(
            LIVE_BYTES.load(Relaxed) > before,
            "bulk::Decompressor is not visible to the global allocator"
        );
    }
    assert_eq!(
        LIVE_BYTES.load(Relaxed),
        before,
        "bulk::Decompressor leaked"
    );
    assert_eq!(input, decompressed);
}

/// Streaming API with plain `Encoder::new` / `Decoder::new`.
fn stream_api_uses_global_allocator() {
    let input = input();

    let before = LIVE_BYTES.load(Relaxed);
    let compressed = {
        let mut encoder = zstd::Encoder::new(Vec::new(), 3).expect("encoder");
        encoder.write_all(&input).expect("compression failed");
        assert!(
            LIVE_BYTES.load(Relaxed) - before > WORKSPACE_FLOOR,
            "stream Encoder workspace is not visible to the global allocator"
        );
        encoder.finish().expect("failed to finish frame")
    };
    // Only the output Vec should still be alive.
    assert_eq!(
        LIVE_BYTES.load(Relaxed) - before,
        compressed.capacity(),
        "stream Encoder leaked"
    );

    let before = LIVE_BYTES.load(Relaxed);
    let mut roundtrip = Vec::with_capacity(input.len());
    {
        let mut decoder =
            zstd::Decoder::new(compressed.as_slice()).expect("decoder");
        decoder
            .read_to_end(&mut roundtrip)
            .expect("decompression failed");
        assert!(
            LIVE_BYTES.load(Relaxed) - before > roundtrip.capacity(),
            "stream Decoder is not visible to the global allocator"
        );
    }
    assert_eq!(
        LIVE_BYTES.load(Relaxed) - before,
        roundtrip.capacity(),
        "stream Decoder leaked"
    );
    assert_eq!(input, roundtrip);
}

/// Dictionary-based streaming (`with_dictionary` loads the dictionary into
/// the context; the prepared dictionaries allocate on their own).
fn dictionaries_use_global_allocator() {
    let input = input();
    let dict: Vec<u8> = (0..=255u8).cycle().take(4096).collect();

    let before = LIVE_BYTES.load(Relaxed);
    let compressed = {
        let mut encoder = zstd::Encoder::with_dictionary(Vec::new(), 3, &dict)
            .expect("encoder");
        encoder.write_all(&input).expect("compression failed");
        assert!(
            LIVE_BYTES.load(Relaxed) - before > WORKSPACE_FLOOR,
            "Encoder::with_dictionary workspace is not visible to the \
             global allocator"
        );
        encoder.finish().expect("failed to finish frame")
    };
    assert_eq!(
        LIVE_BYTES.load(Relaxed) - before,
        compressed.capacity(),
        "Encoder::with_dictionary leaked"
    );

    let before = LIVE_BYTES.load(Relaxed);
    {
        let prepared = zstd::dict::EncoderDictionary::copy(&dict, 3);
        assert!(
            LIVE_BYTES.load(Relaxed) - before >= prepared.as_cdict().sizeof(),
            "EncoderDictionary::copy is not visible to the global allocator"
        );
        let prepared = zstd::dict::DecoderDictionary::copy(&dict);
        assert!(
            LIVE_BYTES.load(Relaxed) - before >= prepared.as_ddict().sizeof(),
            "DecoderDictionary::copy is not visible to the global allocator"
        );
    }
    assert_eq!(
        LIVE_BYTES.load(Relaxed),
        before,
        "prepared dictionaries leaked"
    );

    let before = LIVE_BYTES.load(Relaxed);
    let mut roundtrip = Vec::with_capacity(input.len());
    {
        let prepared = zstd::dict::DecoderDictionary::copy(&dict);
        let mut decoder = zstd::Decoder::with_prepared_dictionary(
            compressed.as_slice(),
            &prepared,
        )
        .expect("decoder");
        decoder
            .read_to_end(&mut roundtrip)
            .expect("decompression failed");
    }
    assert_eq!(
        LIVE_BYTES.load(Relaxed) - before,
        roundtrip.capacity(),
        "Decoder::with_prepared_dictionary leaked"
    );
    assert_eq!(input, roundtrip);

    // One-shot helpers.
    let total = TOTAL_BYTES.load(Relaxed);
    let live = LIVE_BYTES.load(Relaxed);
    let compressed =
        zstd::encode_all(input.as_slice(), 3).expect("encode_all");
    let decoded = zstd::decode_all(compressed.as_slice()).expect("decode_all");
    assert!(TOTAL_BYTES.load(Relaxed) - total > WORKSPACE_FLOOR);
    drop((compressed, decoded));
    assert_eq!(
        LIVE_BYTES.load(Relaxed),
        live,
        "encode_all/decode_all leaked"
    );
}

/// The allocation counters are process-wide, so everything runs from a
/// single test to keep other test threads from disturbing them.
#[test]
fn global_allocator_is_used_by_default() {
    bulk_api_uses_global_allocator();
    stream_api_uses_global_allocator();
    dictionaries_use_global_allocator();
}
