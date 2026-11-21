//! Decoder initialization errors should allow the caller to recover its reader.
//!
//! https://github.com/gyscos/zstd-rs/issues/382

use std::cell::Cell;
use std::io::{self, BufRead, Cursor, Read, Write};
use std::rc::Rc;

const CORRUPT_DICTIONARY: [u8; 16] = [
    0x37, 0xa4, 0x30, 0xec, 0x03, 0x47, 0x2f, 0x23, 0x5b, 0x2d, 0x10, 0x05, 0,
    0, 0, 0,
];

#[derive(Debug)]
struct TrackedReader {
    inner: Cursor<&'static [u8]>,
    calls: usize,
    dropped: Rc<Cell<bool>>,
}

impl Read for TrackedReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.calls += 1;
        self.inner.read(output)
    }
}

impl BufRead for TrackedReader {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.calls += 1;
        self.inner.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.calls += 1;
        self.inner.consume(amount);
    }
}

impl Drop for TrackedReader {
    fn drop(&mut self) {
        self.dropped.set(true);
    }
}

fn tracked_reader() -> (TrackedReader, Rc<Cell<bool>>) {
    let dropped = Rc::new(Cell::new(false));
    let mut inner = Cursor::new(&b"headerrest of archive"[..]);
    inner.set_position(6);
    let reader = TrackedReader {
        inner,
        calls: 0,
        dropped: dropped.clone(),
    };
    (reader, dropped)
}

fn recover<T>(result: Result<T, (TrackedReader, io::Error)>) -> TrackedReader {
    match result {
        Ok(_) => panic!("initialization unexpectedly succeeded"),
        Err((reader, _error)) => reader,
    }
}

fn check_recovered_reader(mut reader: TrackedReader, dropped: Rc<Cell<bool>>) {
    assert!(!dropped.get());
    assert!(Rc::ptr_eq(&reader.dropped, &dropped));
    assert_eq!(reader.calls, 0);
    assert_eq!(reader.inner.position(), 6);

    let mut rest = Vec::new();
    reader.read_to_end(&mut rest).unwrap();
    assert_eq!(rest, b"rest of archive");
    drop(reader);
    assert!(dropped.get());
}

#[test]
fn dictionary_error_returns_the_owned_reader_without_io() {
    let (reader, dropped) = tracked_reader();
    let result =
        zstd::Decoder::try_with_dictionary(reader, &CORRUPT_DICTIONARY);
    check_recovered_reader(recover(result), dropped);
}

#[test]
fn legacy_dictionary_constructor_still_returns_an_io_error() {
    let (reader, dropped) = tracked_reader();
    let result = zstd::Decoder::with_dictionary(reader, &CORRUPT_DICTIONARY);
    assert!(result.is_err());
    assert!(dropped.get());
}

fn decoded<R: Read>(mut decoder: R) -> Vec<u8> {
    let mut output = Vec::new();
    decoder.read_to_end(&mut output).unwrap();
    output
}

#[test]
fn new_and_buffered_constructors_decode_successfully() {
    let payload = b"a valid compressed entry";
    let frame = zstd::encode_all(&payload[..], 3).unwrap();

    let decoder = zstd::Decoder::try_new(Cursor::new(&frame)).unwrap();
    assert_eq!(decoder.get_ref().get_ref().position(), 0);
    assert!(decoder.get_ref().buffer().is_empty());
    assert_eq!(decoded(decoder), payload);

    let decoder = zstd::Decoder::try_with_buffer(&frame[..]).unwrap();
    assert_eq!(decoder.get_ref().len(), frame.len());
    assert_eq!(decoded(decoder), payload);
}

#[test]
fn dictionary_constructors_decode_successfully() {
    let dictionary = b"a raw-content dictionary shared by encoder and decoder";
    let payload = b"some content shared by encoder and decoder";
    let mut encoder = zstd::stream::write::Encoder::with_dictionary(
        Vec::new(),
        3,
        dictionary,
    )
    .unwrap();
    encoder.write_all(payload).unwrap();
    let frame = encoder.finish().unwrap();

    let decoder =
        zstd::Decoder::try_with_dictionary(&frame[..], dictionary).unwrap();
    assert_eq!(decoder.get_ref().len(), frame.len());
    assert_eq!(decoded(decoder), payload);

    let prepared =
        zstd::dict::DecoderDictionary::try_copy(dictionary).unwrap();
    let decoder =
        zstd::Decoder::try_with_prepared_dictionary(&frame[..], &prepared)
            .unwrap();
    assert_eq!(decoder.get_ref().len(), frame.len());
    assert_eq!(decoded(decoder), payload);
}

#[test]
fn prefix_constructor_decodes_successfully() {
    let prefix = b"some shared content for prefix compression";
    let payload = b"some shared content for prefix compression, plus more";
    let mut encoder =
        zstd::stream::write::Encoder::with_ref_prefix(Vec::new(), 3, prefix)
            .unwrap();
    encoder.write_all(payload).unwrap();
    let frame = encoder.finish().unwrap();

    let decoder =
        zstd::Decoder::try_with_ref_prefix(&frame[..], prefix).unwrap();
    assert_eq!(decoder.get_ref().len(), frame.len());
    assert_eq!(decoded(decoder), payload);
}

#[test]
fn invalid_compressed_input_fails_on_read_and_keeps_the_reader() {
    let (reader, dropped) = tracked_reader();
    let mut decoder = zstd::Decoder::try_with_buffer(reader).unwrap();
    assert_eq!(decoder.get_ref().calls, 0);
    assert!(decoder.read(&mut [0u8; 16]).is_err());
    assert!(!dropped.get());
    let calls = decoder.get_ref().calls;
    let reader = decoder.into_inner();
    assert_eq!(reader.calls, calls);
    drop(reader);
    assert!(dropped.get());
}

#[cfg(feature = "with-rust-allocator")]
mod allocation_failure {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};

    thread_local! {
        static FAIL_NEXT: Cell<bool> = const { Cell::new(false) };
        static FAIL_FROM_NEXT: Cell<bool> = const { Cell::new(false) };
        /// How many allocations succeed before one fails (`usize::MAX`: off).
        static FAIL_AFTER: Cell<usize> = const { Cell::new(usize::MAX) };
        /// Once that one failed: how many allocations were attempted since.
        /// They are let through, so that one shows up as a test failure
        /// rather than an abort.
        static SINCE_FAILURE: Cell<Option<usize>> = const { Cell::new(None) };
    }

    /// Whether the allocation being made fails, per `FAIL_AFTER`.
    fn fail_counted() -> bool {
        let since = SINCE_FAILURE.try_with(Cell::get).ok().flatten();
        if let Some(since) = since {
            SINCE_FAILURE.with(|count| count.set(Some(since + 1)));
            return false;
        }
        let left = FAIL_AFTER.try_with(Cell::get).unwrap_or(usize::MAX);
        if left == usize::MAX {
            return false;
        }
        if left > 0 {
            FAIL_AFTER.with(|count| count.set(left - 1));
            return false;
        }
        FAIL_AFTER.with(|count| count.set(usize::MAX));
        SINCE_FAILURE.with(|count| count.set(Some(0)));
        true
    }

    struct FailingAllocator;

    unsafe impl GlobalAlloc for FailingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let fail = fail_counted()
                || FAIL_FROM_NEXT.try_with(Cell::get).unwrap_or(false)
                || FAIL_NEXT
                    .try_with(|flag| flag.replace(false))
                    .unwrap_or(false);
            if fail {
                std::ptr::null_mut()
            } else {
                unsafe { System.alloc(layout) }
            }
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) }
        }
    }

    #[global_allocator]
    static ALLOCATOR: FailingAllocator = FailingAllocator;

    fn fail_next_allocation<T>(constructor: impl FnOnce() -> T) -> T {
        FAIL_NEXT.with(|flag| flag.set(true));
        let result = constructor();
        let pending = FAIL_NEXT.with(|flag| flag.replace(false));
        assert!(
            !pending,
            "constructor did not attempt to allocate a context"
        );
        result
    }

    fn fail_from_next_allocation<T>(constructor: impl FnOnce() -> T) -> T {
        FAIL_FROM_NEXT.with(|flag| flag.set(true));
        let result = constructor();
        FAIL_FROM_NEXT.with(|flag| flag.set(false));
        result
    }

    fn check_out_of_memory<T>(result: io::Result<T>) {
        let error = result.err().expect("constructor should fail");
        assert_eq!(error.kind(), io::ErrorKind::OutOfMemory);
    }

    #[test]
    fn context_allocation_errors_return_the_original_readers() {
        let (reader, dropped) = tracked_reader();
        let result = fail_next_allocation(|| zstd::Decoder::try_new(reader));
        check_recovered_reader(recover(result), dropped);

        let (reader, dropped) = tracked_reader();
        let result =
            fail_next_allocation(|| zstd::Decoder::try_with_buffer(reader));
        check_recovered_reader(recover(result), dropped);

        let dictionary = b"a raw-content dictionary";
        let (reader, dropped) = tracked_reader();
        let result = fail_next_allocation(|| {
            zstd::Decoder::try_with_dictionary(reader, dictionary)
        });
        check_recovered_reader(recover(result), dropped);

        let prepared =
            zstd::dict::DecoderDictionary::try_copy(dictionary).unwrap();
        let (reader, dropped) = tracked_reader();
        let result = fail_next_allocation(|| {
            zstd::Decoder::try_with_prepared_dictionary(reader, &prepared)
        });
        check_recovered_reader(recover(result), dropped);

        let (reader, dropped) = tracked_reader();
        let result = fail_next_allocation(|| {
            zstd::Decoder::try_with_ref_prefix(reader, dictionary)
        });
        check_recovered_reader(recover(result), dropped);
    }

    /// The constructors that make a compression context, and the `bulk`
    /// ones, return the error as well: they used to panic.
    #[test]
    fn context_allocation_errors_are_returned_by_the_other_constructors() {
        use zstd::stream::raw;

        let dictionary = b"a raw-content dictionary";
        let encoder_dictionary =
            zstd::dict::EncoderDictionary::try_copy(dictionary, 3).unwrap();
        let decoder_dictionary =
            zstd::dict::DecoderDictionary::try_copy(dictionary).unwrap();

        let result =
            fail_next_allocation(|| zstd::Encoder::new(io::sink(), 3));
        check_out_of_memory(result);
        let result = fail_next_allocation(|| raw::Encoder::new(3));
        check_out_of_memory(result);
        let result = fail_next_allocation(|| {
            raw::Encoder::with_prepared_dictionary(&encoder_dictionary)
        });
        check_out_of_memory(result);
        let result = fail_next_allocation(|| {
            raw::Encoder::with_ref_prefix(3, dictionary)
        });
        check_out_of_memory(result);

        let result = fail_next_allocation(|| zstd::bulk::Compressor::new(3));
        check_out_of_memory(result);
        let result = fail_next_allocation(|| {
            zstd::bulk::Compressor::with_prepared_dictionary(
                &encoder_dictionary,
            )
        });
        check_out_of_memory(result);

        let result = fail_next_allocation(zstd::bulk::Decompressor::new);
        check_out_of_memory(result);
        let result = fail_next_allocation(|| {
            zstd::bulk::Decompressor::with_prepared_dictionary(
                &decoder_dictionary,
            )
        });
        check_out_of_memory(result);
    }

    #[test]
    fn sticky_allocation_errors_are_returned_without_allocating() {
        use zstd::stream::raw;

        let result =
            fail_from_next_allocation(|| zstd::Encoder::new(io::sink(), 3));
        check_out_of_memory(result);

        let result = fail_from_next_allocation(raw::Decoder::new);
        check_out_of_memory(result);

        let mut compressor = zstd::bulk::Compressor::new(3).unwrap();
        let result =
            fail_from_next_allocation(|| compressor.compress(b"payload"));
        check_out_of_memory(result);
    }

    /// Runs `operation` with its allocation number `skip` (from 0) failing.
    ///
    /// Returns `None` when `operation` made fewer allocations than that.
    /// Panics when it tried to allocate again after the failure: under an
    /// allocator that keeps failing, that would abort the process.
    fn with_failed_allocation<T>(
        name: &str,
        skip: usize,
        operation: impl FnOnce() -> T,
    ) -> Option<T> {
        FAIL_AFTER.with(|count| count.set(skip));
        let result = operation();
        FAIL_AFTER.with(|count| count.set(usize::MAX));
        let since = SINCE_FAILURE.with(|count| count.replace(None))?;
        assert_eq!(
            since, 0,
            "{}: allocated again after allocation {} failed",
            name, skip
        );
        Some(result)
    }

    /// Fails each allocation `operation` makes in turn, and checks that it
    /// then returns an `OutOfMemory` error without allocating again.
    fn check_every_allocation<T>(
        name: &str,
        mut operation: impl FnMut() -> io::Result<T>,
    ) {
        for skip in 0.. {
            let result =
                match with_failed_allocation(name, skip, &mut operation) {
                    Some(result) => result,
                    None => {
                        assert!(skip > 0, "{}: nothing was allocated", name);
                        return;
                    }
                };
            let kind = result.err().map(|error| error.kind());
            assert_eq!(
                kind,
                Some(io::ErrorKind::OutOfMemory),
                "{}: allocation {} failed",
                name,
                skip
            );
        }
    }

    #[test]
    fn every_allocation_failure_is_returned_without_allocating() {
        use zstd::stream::raw;

        let data: Vec<u8> = (0..64 << 10).map(|i| (i % 251) as u8).collect();
        let dictionary = &data[..4096];
        let frame = zstd::bulk::compress(&data, 3).unwrap();
        let mut output =
            vec![0u8; zstd::zstd_safe::compress_bound(data.len())];

        check_every_allocation("bulk::compress", || {
            zstd::bulk::compress(&data, 19)
        });
        check_every_allocation("Compressor::compress_to_buffer", || {
            zstd::bulk::Compressor::new(19)?
                .compress_to_buffer(&data, &mut output[..])
        });
        check_every_allocation("Compressor::with_dictionary", || {
            zstd::bulk::Compressor::with_dictionary(19, dictionary)?
                .compress_to_buffer(&data, &mut output[..])
        });
        check_every_allocation("write::Encoder", || {
            let mut encoder = zstd::Encoder::new(io::sink(), 19)?;
            encoder.write_all(&data)?;
            encoder.finish()
        });
        check_every_allocation("write::Encoder::with_dictionary", || {
            let mut encoder =
                zstd::Encoder::with_dictionary(io::sink(), 19, dictionary)?;
            encoder.write_all(&data)?;
            encoder.finish()
        });
        // Prepared outside: dictionaries report running out of memory as
        // an invalid dictionary.
        let prepared =
            zstd::dict::EncoderDictionary::try_copy(dictionary, 19).unwrap();
        check_every_allocation("Compressor::with_prepared_dictionary", || {
            zstd::bulk::Compressor::with_prepared_dictionary(&prepared)?
                .compress_to_buffer(&data, &mut output[..])
        });
        check_every_allocation("stream::copy_encode", || {
            zstd::stream::copy_encode(&data[..], io::sink(), 19)
        });
        check_every_allocation("bulk::decompress", || {
            zstd::bulk::decompress(&frame, data.len())
        });
        // The Rust port (libzstd-rs-sys 0.0.1-prerelease.2) reports a
        // failure to allocate the stream's buffers as `dictionary_corrupted`
        // (trifectatechfoundation/libzstd-rs-sys#559).
        if !cfg!(feature = "rust-backend") {
            check_every_allocation("write::Decoder", || {
                let mut decoder =
                    zstd::stream::write::Decoder::new(io::sink())?;
                decoder.write_all(&frame)?;
                decoder.flush()
            });
        }
        let dict_frame =
            zstd::bulk::Compressor::with_dictionary(3, dictionary)
                .unwrap()
                .compress(&data)
                .unwrap();
        check_every_allocation("Decompressor::with_dictionary", || {
            zstd::bulk::Decompressor::with_dictionary(dictionary)?
                .decompress(&dict_frame, data.len())
        });
        if !cfg!(feature = "rust-backend") {
            check_every_allocation("write::Decoder::with_dictionary", || {
                let mut decoder =
                    zstd::stream::write::Decoder::with_dictionary(
                        io::sink(),
                        dictionary,
                    )?;
                decoder.write_all(&dict_frame)?;
                decoder.flush()
            });
        }
        check_every_allocation("raw::Decoder::with_dictionary", || {
            raw::Decoder::with_dictionary(dictionary)
        });
    }
}
