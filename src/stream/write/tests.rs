use std::io::{Cursor, Write};
use std::iter;

use partial_io::{PartialOp, PartialWrite};

use crate::stream::decode_all;
use crate::stream::write::{Decoder, Encoder};

#[test]
fn test_cycle() {
    let input = b"Abcdefghabcdefgh";

    let buffer = Cursor::new(Vec::new());
    let mut encoder = Encoder::new(buffer, 1).unwrap();
    encoder.write_all(input).unwrap();
    let encoded = encoder.finish().unwrap().into_inner();

    // println!("Encoded: {:?}", encoded);

    let buffer = Cursor::new(Vec::new());
    let mut decoder = Decoder::new(buffer).unwrap();
    decoder.write_all(&encoded).unwrap();
    decoder.flush().unwrap();
    let decoded = decoder.into_inner().into_inner();

    assert_eq!(input, &decoded[..]);
}

/// Test that flush after a partial write works successfully without
/// corrupting the frame. This test is in this module because it checks
/// internal implementation details.
#[test]
fn test_partial_write_flush() {
    let input = vec![b'b'; 128 * 1024];
    let mut z = setup_partial_write(&input);

    // flush shouldn't corrupt the stream
    z.flush().unwrap();

    let buf = z.finish().unwrap().into_inner();
    assert_eq!(&decode_all(&buf[..]).unwrap(), &input);
}

/// Test that finish after a partial write works successfully without
/// corrupting the frame. This test is in this module because it checks
/// internal implementation details.
#[test]
fn test_partial_write_finish() {
    let input = vec![b'b'; 128 * 1024];
    let z = setup_partial_write(&input);

    // finish shouldn't corrupt the stream
    let buf = z.finish().unwrap().into_inner();
    assert_eq!(&decode_all(&buf[..]).unwrap(), &input);
}

fn setup_partial_write(
    input_data: &[u8],
) -> Encoder<'_, PartialWrite<Vec<u8>>> {
    let buf =
        PartialWrite::new(Vec::new(), iter::repeat(PartialOp::Limited(1)));
    let mut z = Encoder::new(buf, 1).unwrap();

    // Fill in enough data to make sure the buffer gets written out.
    z.write_all(input_data).unwrap();

    {
        let inner = &mut z.writer;
        // At this point, the internal buffer in z should have some data.
        assert_ne!(inner.offset(), inner.buffer().len());
    }

    z
}

/// Test that a panic from the writer can be caught: the wrappers must not hand
/// it the same data again on drop, or the second panic aborts the process.
#[test]
fn test_auto_wrappers_panicking_writer() {
    struct Panicking;

    impl Write for Panicking {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            panic!("this writer panics");
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let input = vec![b'b'; 128 * 1024];
    let encoded = crate::encode_all(&input[..], 1).unwrap();

    std::panic::catch_unwind(|| {
        let mut decoder = Decoder::new(Panicking).unwrap().auto_flush();
        decoder.write_all(&encoded)
    })
    .unwrap_err();

    std::panic::catch_unwind(|| {
        let mut encoder = Encoder::new(Panicking, 1).unwrap().auto_finish();
        encoder.write_all(&input).and_then(|()| encoder.flush())
    })
    .unwrap_err();
}
