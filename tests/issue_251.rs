use std::io::Read;

#[test]
fn test_issue_251() {
    // This is 64 compressed zero bytes.
    let compressed_data = include_bytes!("../assets/zeros64.zst");
    let decompressed_size = 64;

    // Construct a decompressor using `with_buffer`.  This should be ok as
    // `Cursor` is `BufRead`.
    let reader = std::io::Cursor::new(compressed_data);
    let mut decomp = zstd::Decoder::with_buffer(reader).unwrap();

    // Count how many bytes we decompress.
    let mut total = 0;

    // Decompress four bytes at a time (this is necessary to trigger underrun
    // behaviour).
    for _ in 0..(decompressed_size / 4) {
        let mut buf = [0u8; 4];
        let count = decomp.read(&mut buf).unwrap();
        total += count;
    }

    // Finish reading and get the buffer back.
    let reader = decomp.finish();

    // The cursor should now be at the end of the compressed data.
    println!("We read {total}/{decompressed_size} decompressed bytes");
    println!(
        "The underlying cursor is at position {} of {} compressed bytes",
        reader.position(),
        compressed_data.len()
    );

    assert_eq!(total, decompressed_size);
    assert_eq!(reader.position() as usize, compressed_data.len());
}
