//! A dictionary zstd cannot load: the dictionary magic number, then a corrupt header.
//! `ZSTD_createCDict` / `ZSTD_createDDict` return NULL for it.

use zstd::dict::{DecoderDictionary, EncoderDictionary};

const CORRUPT: [u8; 16] = [
    0x37, 0xa4, 0x30, 0xec, 0x03, 0x47, 0x2f, 0x23, 0x5b, 0x2d, 0x10, 0x05, 0,
    0, 0, 0,
];

#[test]
fn try_copy_rejects_an_invalid_dictionary() {
    assert!(EncoderDictionary::try_copy(&CORRUPT, 3).is_err());
    assert!(DecoderDictionary::try_copy(&CORRUPT).is_err());
}

#[cfg(feature = "experimental")]
#[test]
fn try_new_rejects_an_invalid_dictionary() {
    assert!(EncoderDictionary::try_new(&CORRUPT, 3).is_err());
    assert!(DecoderDictionary::try_new(&CORRUPT).is_err());
}

#[test]
fn try_copy_accepts_a_valid_dictionary() {
    let dict = b"a raw-content dictionary: any bytes without the magic number";
    let encoder = EncoderDictionary::try_copy(dict, 3).unwrap();
    let decoder = DecoderDictionary::try_copy(dict).unwrap();

    let data = b"some data that shares words with the dictionary";
    let frame = zstd::bulk::Compressor::with_prepared_dictionary(&encoder)
        .unwrap()
        .compress(data)
        .unwrap();
    let back = zstd::bulk::Decompressor::with_prepared_dictionary(&decoder)
        .unwrap()
        .decompress(&frame, data.len())
        .unwrap();
    assert_eq!(back, data);
}

#[test]
#[should_panic]
fn copy_still_panics_on_an_invalid_encoder_dictionary() {
    let _ = EncoderDictionary::copy(&CORRUPT, 3);
}

#[test]
#[should_panic]
fn copy_still_panics_on_an_invalid_decoder_dictionary() {
    let _ = DecoderDictionary::copy(&CORRUPT);
}
