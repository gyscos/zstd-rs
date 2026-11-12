use super::{compress, decompress};

const TEXT: &str = include_str!("../../assets/example.txt");

/// Compresses `data` into a frame that does not record its size, as a
/// streaming encoder that is never told the length produces.
fn sizeless(data: &[u8]) -> Vec<u8> {
    use std::io::Write;

    let mut encoder =
        crate::stream::write::Encoder::new(Vec::new(), 1).unwrap();
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

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
    // A streaming compressor that is not told the length has nothing to
    // write in the header.
    let compressed = sizeless(TEXT.as_bytes());

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
    mixed.extend_from_slice(&sizeless(TEXT.as_bytes()));

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

#[test]
fn test_decompress_capacity_below_the_known_size() {
    let compressed = compress(TEXT.as_bytes(), 1).unwrap();

    // The frame records its size, so this is known to be impossible before
    // zstd is asked to try, and the error names both numbers.
    let error = decompress(&compressed, TEXT.len() / 2).unwrap_err();
    let message = error.to_string();

    assert!(message.contains(&TEXT.len().to_string()), "{}", message);
    assert!(
        message.contains(&(TEXT.len() / 2).to_string()),
        "{}",
        message
    );
}

#[test]
fn test_decompress_capacity_between_actual_and_bound() {
    // A frame that records nothing: only an over-estimate is available, and
    // refusing on that would reject this, which decompresses perfectly well.
    let compressed = sizeless(TEXT.as_bytes());
    assert_eq!(
        zstd_safe::get_frame_content_size(&compressed).unwrap(),
        None
    );

    let generous = TEXT.len() * 2;
    assert_eq!(decompress(&compressed, generous).unwrap().len(), TEXT.len());
}

/// A capacity that cannot be allocated is an error, not an abort or a panic.
#[test]
fn test_decompress_impossible_capacity() {
    let error = super::decompress(&[0u8], usize::MAX).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::OutOfMemory);
}

/// A job that fails must not leave the compressor refusing to be set up for
/// the next one.
#[test]
fn test_compressor_is_reusable_after_a_failed_job() {
    let dictionary = crate::dict::EncoderDictionary::copy(b"another", 3);
    let text = TEXT.as_bytes();

    let mut compressor = super::Compressor::new(1).unwrap();
    let fail = |compressor: &mut super::Compressor<'_>| {
        let mut small = Vec::with_capacity(16);
        compressor.compress_to_buffer(text, &mut small).unwrap_err();
    };

    fail(&mut compressor);
    compressor
        .set_pledged_src_size(Some(text.len() as u64))
        .unwrap();
    compressor.include_checksum(true).unwrap();
    compressor.set_compression_level(3).unwrap();

    fail(&mut compressor);
    compressor.set_dictionary(3, b"some dictionary").unwrap();

    fail(&mut compressor);
    compressor.set_prepared_dictionary(&dictionary).unwrap();

    fail(&mut compressor);
    compressor.set_dictionary(1, &[]).unwrap();

    let compressed = compressor.compress(text).unwrap();
    assert_eq!(decompress(&compressed, text.len()).unwrap(), text);
}

/// With worker threads, a failed job may leave them reading the dictionary:
/// replacing it right after waits for them.
#[cfg(feature = "zstdmt")]
#[test]
fn test_multithreaded_compressor_after_a_failed_job() {
    use crate::test_words as words;

    let dictionary = words(1 << 20, 1);
    let another = words(64 << 10, 2);
    let input = words(24 << 20, 3);

    for _ in 0..3 {
        let mut compressor =
            super::Compressor::with_dictionary(1, &dictionary).unwrap();
        compressor.multithread(2).unwrap();
        compressor
            .compress_to_buffer(&input, &mut Vec::with_capacity(0))
            .unwrap_err();
        compressor.set_dictionary(1, &another).unwrap();
        compressor.multithread(1).unwrap();

        let compressed = compressor.compress(&input).unwrap();
        let mut decompressor =
            super::Decompressor::with_dictionary(&another).unwrap();
        assert!(
            decompressor.decompress(&compressed, input.len()).unwrap()
                == input
        );

        // Dropping it in the middle of a failed job is fine too.
        compressor
            .compress_to_buffer(&input, &mut Vec::with_capacity(0))
            .unwrap_err();
    }
}
