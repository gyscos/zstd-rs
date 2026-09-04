//! A reader that stops as soon as it has the decoded bytes it wanted leaves
//! zstd holding the tail of the frame: the frame epilogue has not been read
//! from the input yet. `finish()` should hand back a reader lined up with the
//! end of the frame - and nothing more than that.
//!
//! https://github.com/gyscos/zstd-rs/issues/251

use std::cell::Cell;
use std::io::{self, BufRead, Cursor, Read};

const PAYLOAD_LEN: usize = 64;

fn frame() -> Vec<u8> {
    zstd::encode_all(&vec![0u8; PAYLOAD_LEN][..], 3).unwrap()
}

/// Read exactly `n` decoded bytes, the way a caller who knows the decompressed
/// size does. This is what leaves the frame epilogue unread.
fn read_exactly<R: BufRead>(decoder: &mut zstd::Decoder<'_, R>, n: usize) {
    let mut got = 0;
    while got < n {
        let mut buf = [0u8; 8];
        match decoder.read(&mut buf).unwrap() {
            0 => break,
            read => got += read,
        }
    }
    assert_eq!(got, n, "did not decode the whole payload");
}

#[test]
fn finish_leaves_the_reader_at_the_end_of_the_frame() {
    let frame = frame();
    let mut decoder = zstd::Decoder::with_buffer(Cursor::new(&frame)).unwrap();
    read_exactly(&mut decoder, PAYLOAD_LEN);

    let reader = decoder.finish();
    assert_eq!(reader.position() as usize, frame.len());
}

#[test]
fn trailing_data_is_left_for_the_caller() {
    const TRAILER: &[u8] = b"trailing container data";
    let mut stream = frame();
    let frame_len = stream.len();
    stream.extend_from_slice(TRAILER);

    let mut decoder =
        zstd::Decoder::with_buffer(Cursor::new(&stream)).unwrap();
    read_exactly(&mut decoder, PAYLOAD_LEN);

    let mut reader = decoder.finish();
    assert_eq!(reader.position() as usize, frame_len);

    let mut rest = Vec::new();
    reader.read_to_end(&mut rest).unwrap();
    assert_eq!(rest, TRAILER);
}

#[test]
fn concatenated_frames_are_not_consumed() {
    let first = frame();
    let second = zstd::encode_all(&vec![7u8; PAYLOAD_LEN][..], 3).unwrap();
    let mut stream = first.clone();
    stream.extend_from_slice(&second);

    let mut decoder =
        zstd::Decoder::with_buffer(Cursor::new(&stream)).unwrap();
    read_exactly(&mut decoder, PAYLOAD_LEN);

    let reader = decoder.finish();
    assert_eq!(
        reader.position() as usize,
        first.len(),
        "finish() ran past the end of the first frame"
    );

    // The second frame is still intact and decodable.
    let rest = &stream[first.len()..];
    assert_eq!(
        zstd::decode_all(rest).unwrap(),
        vec![7u8; PAYLOAD_LEN],
        "the second frame did not survive finish()"
    );
}

/// A `BufRead` that counts how often it is asked for more data.
struct Counting<R> {
    inner: R,
    fill_buf_calls: Cell<usize>,
}

impl<R> Counting<R> {
    fn new(inner: R) -> Self {
        Counting {
            inner,
            fill_buf_calls: Cell::new(0),
        }
    }
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: BufRead> BufRead for Counting<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.fill_buf_calls.set(self.fill_buf_calls.get() + 1);
        self.inner.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.inner.consume(amt)
    }
}

#[test]
fn finish_does_not_read_when_the_frame_is_complete() {
    let frame = frame();
    let mut decoder =
        zstd::Decoder::with_buffer(Counting::new(Cursor::new(&frame)))
            .unwrap();

    // Reading to the end already consumes the epilogue.
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).unwrap();
    assert_eq!(decoded.len(), PAYLOAD_LEN);

    // From here on the underlying reader must be left alone: it could be a
    // socket with nothing more to give, and asking would block forever.
    let before = decoder.get_ref().fill_buf_calls.get();
    let reader = decoder.finish();
    assert_eq!(
        reader.fill_buf_calls.get(),
        before,
        "finish() read from the underlying reader although the frame was \
         already complete"
    );
}

#[test]
fn finish_on_a_partially_read_stream_does_not_hang() {
    // The caller gives up half-way: there is far more frame left than we can
    // flush into no output at all, so finish() has to give up rather than spin.
    let big = zstd::encode_all(&vec![0u8; 1 << 20][..], 3).unwrap();
    let mut decoder = zstd::Decoder::with_buffer(Cursor::new(&big)).unwrap();
    decoder.read_exact(&mut [0u8; 4]).unwrap();

    let reader = decoder.finish();
    assert!(reader.position() as usize <= big.len());
}

#[test]
fn finish_does_not_read_ahead_past_a_completed_frame() {
    // One big read can consume the whole frame, epilogue included. There is
    // more data right behind it, so a finish() that pulls on the reader would
    // get something - it must not.
    const TRAILER: &[u8] = b"trailing container data";
    let mut stream = frame();
    let frame_len = stream.len();
    stream.extend_from_slice(TRAILER);

    let mut decoder =
        zstd::Decoder::with_buffer(Counting::new(Cursor::new(&stream)))
            .unwrap();
    let mut buf = [0u8; 4096];
    let read = decoder.read(&mut buf).unwrap();
    assert_eq!(read, PAYLOAD_LEN, "expected the payload in a single read");

    let before = decoder.get_ref().fill_buf_calls.get();
    let reader = decoder.finish();
    assert_eq!(reader.inner.position() as usize, frame_len);
    assert_eq!(
        reader.fill_buf_calls.get(),
        before,
        "finish() read ahead although the frame was already complete"
    );
}
