//! Recovering a decoder's reader must not consume compressed input.
//!
//! https://github.com/gyscos/zstd-rs/issues/381

use std::cell::Cell;
use std::io::{self, BufRead, Cursor, Read};

struct Counting<R> {
    inner: R,
    fill_buf_calls: Cell<usize>,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.inner.read(output)
    }
}

impl<R: BufRead> BufRead for Counting<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.fill_buf_calls.set(self.fill_buf_calls.get() + 1);
        self.inner.fill_buf()
    }

    fn consume(&mut self, amount: usize) {
        self.inner.consume(amount);
    }
}

#[test]
fn into_inner_preserves_an_unread_frame_for_raw_copying() {
    let payload = vec![0u8; 64];
    let frame = zstd::encode_all(&payload[..], 3).unwrap();
    let source = Counting {
        inner: Cursor::new(&frame),
        fill_buf_calls: Cell::new(0),
    };
    let decoder = zstd::Decoder::with_buffer(source).unwrap();

    let mut reader = decoder.into_inner();
    assert_eq!(reader.inner.position(), 0);
    assert_eq!(reader.fill_buf_calls.get(), 0);

    let mut copied = Vec::new();
    io::copy(&mut reader, &mut copied).unwrap();
    assert_eq!(copied, frame);
    assert_eq!(zstd::decode_all(&copied[..]).unwrap(), payload);
}

#[test]
fn into_inner_does_not_fill_an_unread_bufreader() {
    let frame = zstd::encode_all(&b"raw copy"[..], 3).unwrap();
    let decoder = zstd::Decoder::new(Cursor::new(&frame)).unwrap();

    let reader = decoder.into_inner();
    assert_eq!(reader.get_ref().position(), 0);
    assert!(reader.buffer().is_empty());
}

#[test]
fn into_inner_preserves_the_position_after_a_partial_read() {
    let frame = zstd::encode_all(&vec![0u8; 1 << 20][..], 3).unwrap();
    let source = Counting {
        inner: Cursor::new(&frame),
        fill_buf_calls: Cell::new(0),
    };
    let mut decoder = zstd::Decoder::with_buffer(source).unwrap();
    decoder.read_exact(&mut [0u8; 4]).unwrap();
    let position = decoder.get_ref().inner.position();
    let calls = decoder.get_ref().fill_buf_calls.get();

    let reader = decoder.into_inner();
    assert_eq!(reader.inner.position(), position);
    assert_eq!(reader.fill_buf_calls.get(), calls);
}
