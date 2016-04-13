//! Methods to compress and decompress individual blocks.
//!
//! These methods process all the input data at once.
//! It is therefore best used with relatively small blocks
//! (like small network packets).

use ll;

use std::io;

/// Compress a single block of data to the given destination buffer.
///
/// Returns the number of bytes written, or an error if something happened
/// (for instance if the destination buffer was too small).
pub fn compress_to_buffer(destination: &mut [u8], source: &[u8], level: i32)
                          -> io::Result<usize> {
    let code = unsafe {
        ll::ZSTD_compress(destination.as_mut_ptr(),
                          destination.len(),
                          source.as_ptr(),
                          source.len(),
                          level)
    };
    ll::parse_code(code)
}

/// Compress a block of data, and return the compressed result in a `Vec<u8>`.
pub fn compress(data: &[u8], level: i32) -> io::Result<Vec<u8>> {
    // We allocate a big buffer, slightly larger than the input data.
    let buffer_len = unsafe { ll::ZSTD_compressBound(data.len()) };
    let mut buffer = Vec::with_capacity(buffer_len);

    unsafe {
        // Use all capacity. Memory may not be initialized, but we won't read it.
        buffer.set_len(buffer_len);
        let len = try!(compress_to_buffer(&mut buffer[..], data, level));
        buffer.set_len(len);
    }

    // Should we shrink the vec? Meh, let the user do it if he wants.
    Ok(buffer)
}

/// Deompress a single block of data to the given destination buffer.
///
/// Returns the number of bytes written, or an error if something happened
/// (for instance if the destination buffer was too small).
pub fn decompress_to_buffer(destination: &mut [u8], source: &[u8])
                            -> io::Result<usize> {
    let code = unsafe {
        ll::ZSTD_decompress(destination.as_mut_ptr(),
                            destination.len(),
                            source.as_ptr(),
                            source.len())
    };
    ll::parse_code(code)
}

/// Decompress a block of data, and return the decompressed result in a `Vec<u8>`.
///
/// The decompressed data should be less than `capacity` bytes,
/// or an error will be returned.
pub fn decompress(data: &[u8], capacity: usize) -> io::Result<Vec<u8>> {
    let mut buffer = Vec::with_capacity(capacity);
    unsafe {
        buffer.set_len(capacity);
        let len = try!(decompress_to_buffer(&mut buffer[..], data));
        buffer.set_len(len);
    }
    Ok(buffer)
}

#[test]
fn test_direct() {
    // hipsum.co
    let text = "Pork belly art party wolf XOXO, neutra scenester ugh \
                thundercats tattooed squid skateboard beard readymade kogi. \
                VHS cardigan schlitz, meditation chartreuse kogi tilde \
                church-key. Actually direct trade hammock, aesthetic VHS \
                semiotics organic narwhal lo-fi heirloom flexitarian master \
                cleanse polaroid man bun. Flannel helvetica mustache, \
                bicycle rights small batch slow-carb neutra tilde \
                williamsburg meh poutine humblebrag. Four dollar toast \
                butcher actually franzen, gastropub mustache tofu cardigan. \
                90's fingerstache forage brooklyn meditation single-origin \
                coffee tofu actually, ramps pabst farm-to-table art party \
                kombucha artisan fanny pack. Flannel salvia ennui viral \
                leggings selfies.";

    let compressed = compress(text.as_bytes(), 1).unwrap();

    let uncompressed = decompress(&compressed, text.len()).unwrap();

    assert_eq!(text.as_bytes(), &uncompressed[..]);
}
