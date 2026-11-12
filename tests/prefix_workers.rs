//! Delta encoding against a base: a borrowed prefix gives up the worker
//! threads while streaming; an owned one, or a one-shot call, keeps them.
#![cfg(feature = "zstdmt")]

use std::io::{Read, Write};
use std::sync::Arc;

/// Compressible data, different for each seed.
fn data(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            b"lorem ipsum dolor sit amet "[(x % 27) as usize]
        })
        .collect()
}

fn decode(compressed: &[u8], base: &[u8]) -> Vec<u8> {
    let mut decoded = Vec::new();
    zstd::stream::read::Decoder::with_ref_prefix(compressed, base)
        .unwrap()
        .read_to_end(&mut decoded)
        .unwrap();
    decoded
}

/// Streams `new` through `encoder`, with `workers`.
fn stream(
    mut encoder: zstd::Encoder<'_, Vec<u8>>,
    workers: u32,
    new: &[u8],
) -> Vec<u8> {
    encoder.multithread(workers).unwrap();
    encoder.write_all(new).unwrap();
    encoder.finish().unwrap()
}

#[test]
fn owned_prefixes_keep_the_workers() {
    let base: Arc<[u8]> = data(1 << 20, 1).into();
    let new = data(4 << 20, 2);
    let borrowed = |workers| {
        stream(
            zstd::Encoder::with_ref_prefix(Vec::new(), 1, &base).unwrap(),
            workers,
            &new,
        )
    };
    let single = borrowed(0);
    assert!(borrowed(2) == single, "a borrowed prefix used the workers");
    let owned = stream(
        zstd::Encoder::with_prefix(Vec::new(), 1, base.clone()).unwrap(),
        2,
        &new,
    );
    assert!(owned != single, "an owned prefix did not use the workers");

    let mut compressor = zstd::bulk::Compressor::new(1).unwrap();
    compressor.multithread(2).unwrap();
    let one_shot = compressor.compress_with_prefix(&new, &base).unwrap();
    let mut single_shot = zstd::bulk::Compressor::new(1).unwrap();
    assert!(
        one_shot != single_shot.compress_with_prefix(&new, &base).unwrap()
    );

    for compressed in [&single, &owned, &one_shot] {
        assert!(decode(compressed, &base) == new);
    }
}
