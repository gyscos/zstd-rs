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
    }

    struct FailOnceAllocator;

    unsafe impl GlobalAlloc for FailOnceAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if FAIL_NEXT
                .try_with(|flag| flag.replace(false))
                .unwrap_or(false)
            {
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
    static ALLOCATOR: FailOnceAllocator = FailOnceAllocator;

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
}
