use super::{compress, decompress};

const TEXT: &str = include_str!("../../assets/example.txt");

#[test]
fn test_direct() {
    // Can we include_str!("assets/example.txt")?
    // It's excluded from the packaging step, so maybe not.
    crate::test_cycle_unwrap(
        TEXT.as_bytes(),
        |data| compress(data, 1),
        |data| decompress(data, TEXT.len()),
    );
}

#[test]
fn test_stream_compat() {
    // We can bulk-compress and stream-decode
    crate::test_cycle_unwrap(
        TEXT.as_bytes(),
        |data| compress(data, 1),
        |data| crate::decode_all(data),
    );

    // We can stream-encode and bulk-decompress
    crate::test_cycle_unwrap(
        TEXT.as_bytes(),
        |data| crate::encode_all(data, 1),
        |data| decompress(data, TEXT.len()),
    );
}

#[test]
fn has_content_size() {
    let compressed = compress(TEXT.as_bytes(), 1).unwrap();

    // Bulk functions by default include the content size.
    assert_eq!(
        zstd_safe::get_frame_content_size(&compressed).unwrap(),
        Some(TEXT.len() as u64)
    );
}

#[test]
fn test_upper_bound_single_frame() {
    // bulk::compress knows the length, so the frame records it and the bound
    // is the exact size rather than a guess.
    let compressed = compress(TEXT.as_bytes(), 1).unwrap();

    assert_eq!(
        super::Decompressor::upper_bound(&compressed),
        Some(TEXT.len())
    );
}

#[test]
fn test_upper_bound_without_a_recorded_size() {
    // A streaming compressor is not told the length, so there is nothing to
    // read out of the header.
    let compressed = crate::encode_all(TEXT.as_bytes(), 1).unwrap();

    assert_eq!(
        zstd_safe::get_frame_content_size(&compressed).unwrap(),
        None
    );

    // Without the experimental API there is nothing to go on, so we say so
    // and the caller's capacity gets used. With it, zstd still offers a bound
    // worked out from the window size - an over-estimate, but a real bound.
    #[cfg(not(feature = "experimental"))]
    assert_eq!(super::Decompressor::upper_bound(&compressed), None);

    #[cfg(feature = "experimental")]
    {
        let bound = super::Decompressor::upper_bound(&compressed)
            .expect("zstd bounds it from the window size");
        assert!(bound >= TEXT.len(), "{} is not an upper bound", bound);
    }
}

#[test]
fn test_upper_bound_concatenated_frames() {
    let mut both = compress(TEXT.as_bytes(), 1).unwrap();
    both.extend_from_slice(&compress(TEXT.as_bytes(), 1).unwrap());

    // Summing them needs no experimental API: we walk the frames ourselves.
    assert_eq!(
        super::Decompressor::upper_bound(&both),
        Some(TEXT.len() * 2)
    );

    // Either way the data round-trips.
    assert_eq!(
        decompress(&both, TEXT.len() * 2).unwrap().len(),
        TEXT.len() * 2
    );
}

#[test]
fn test_upper_bound_of_nonsense() {
    // Not a frame at all: no answer either way.
    assert_eq!(super::Decompressor::upper_bound(&[0xFF; 32]), None);
}

#[test]
fn test_upper_bound_many_frames() {
    // Walking the frames has to keep up with however many there are.
    let mut joined = Vec::new();
    for _ in 0..8 {
        joined.extend_from_slice(&compress(TEXT.as_bytes(), 1).unwrap());
    }

    assert_eq!(
        super::Decompressor::upper_bound(&joined),
        Some(TEXT.len() * 8)
    );
    assert_eq!(
        decompress(&joined, TEXT.len() * 8).unwrap().len(),
        TEXT.len() * 8
    );
}

#[test]
fn test_upper_bound_stops_at_a_frame_without_a_size() {
    // One good frame followed by one that records nothing: no total to give.
    let mut mixed = compress(TEXT.as_bytes(), 1).unwrap();
    mixed.extend_from_slice(&crate::encode_all(TEXT.as_bytes(), 1).unwrap());

    #[cfg(not(feature = "experimental"))]
    assert_eq!(super::Decompressor::upper_bound(&mixed), None);

    // With the experimental API, zstd bounds the sizeless frame by its window.
    #[cfg(feature = "experimental")]
    assert!(
        super::Decompressor::upper_bound(&mixed).unwrap() >= TEXT.len() * 2
    );
}

#[test]
fn test_upper_bound_of_a_truncated_frame() {
    let compressed = compress(TEXT.as_bytes(), 1).unwrap();
    let truncated = &compressed[..compressed.len() - 1];

    assert_eq!(super::Decompressor::upper_bound(truncated), None);
}

#[test]
fn test_upper_bound_with_a_skippable_frame() {
    // Skippable frames carry no content, and zstd reports them as such, so
    // walking the frames steps over them and the total stays exact.
    let mut data = Vec::new();
    data.extend_from_slice(&0x184D2A50u32.to_le_bytes()); // skippable magic
    data.extend_from_slice(&4u32.to_le_bytes()); // its length
    data.extend_from_slice(b"meta");
    data.extend_from_slice(&compress(TEXT.as_bytes(), 1).unwrap());

    assert_eq!(super::Decompressor::upper_bound(&data), Some(TEXT.len()));
    assert_eq!(decompress(&data, TEXT.len()).unwrap().len(), TEXT.len());
}
