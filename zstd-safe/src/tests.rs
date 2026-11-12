extern crate std;
use crate as zstd_safe;

use self::std::vec::Vec;

const INPUT: &[u8] = b"Rust is a multi-paradigm system programming language focused on safety, especially safe concurrency. Rust is syntactically similar to C++, but is designed to provide better memory safety while maintaining high performance.";
const LONG_CONTENT: &str = include_str!("lib.rs");

#[cfg(feature = "std")]
#[test]
fn test_writebuf() {
    use zstd_safe::WriteBuf;

    let mut data = Vec::with_capacity(10);
    unsafe {
        data.write_from(|ptr, n| {
            assert!(n >= 4);
            let ptr = ptr as *mut u8;
            ptr.write(0);
            ptr.add(1).write(1);
            ptr.add(2).write(2);
            ptr.add(3).write(3);
            Ok(4)
        })
    }
    .unwrap();
    assert_eq!(data.as_slice(), &[0, 1, 2, 3]);

    let mut cursor = std::io::Cursor::new(&mut data);
    // Here we use a position larger than the actual data.
    // So expect the data to be zero-filled.
    cursor.set_position(6);
    unsafe {
        cursor.write_from(|ptr, n| {
            assert!(n >= 4);
            let ptr = ptr as *mut u8;
            ptr.write(4);
            ptr.add(1).write(5);
            ptr.add(2).write(6);
            ptr.add(3).write(7);
            Ok(4)
        })
    }
    .unwrap();

    assert_eq!(data.as_slice(), &[0, 1, 2, 3, 0, 0, 4, 5, 6, 7]);
}

/// `Vec` is a `WriteBuf` with just `alloc`. Decompressing into one leaves
/// its length at the decompressed size, even with plenty of spare capacity:
/// zstd may scribble past the output, but only in that spare capacity.
#[cfg(feature = "alloc")]
#[test]
fn test_vec_round_trip_without_std() {
    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(INPUT.len()));
    zstd_safe::compress(&mut compressed, INPUT, 3).unwrap();

    let mut decompressed = Vec::with_capacity(1 << 20);
    let written =
        zstd_safe::decompress(&mut decompressed, &compressed).unwrap();
    assert_eq!(written, INPUT.len());
    assert_eq!(decompressed, INPUT);
}

/// Writing nothing after a cursor truncates there, like writing something
/// does: it must not leave the previous result in place.
#[cfg(feature = "std")]
#[test]
fn test_cursor_empty_write_truncates() {
    use zstd_safe::DCtx;

    let mut dctx = DCtx::create();
    let mut frame = std::vec![0u8; 128];
    let frame_len =
        zstd_safe::compress(&mut frame[..], b"previous secret", 1).unwrap();
    let mut empty = std::vec![0u8; 128];
    let empty_len = zstd_safe::compress(&mut empty[..], b"", 1).unwrap();

    let mut buffer = Vec::with_capacity(64);
    buffer.extend_from_slice(b"> ");
    let mut cursor = std::io::Cursor::new(&mut buffer);
    cursor.set_position(2);
    dctx.decompress(&mut cursor, &frame[..frame_len]).unwrap();
    assert_eq!(cursor.get_ref().as_slice(), b"> previous secret");

    cursor.set_position(2);
    assert_eq!(dctx.decompress(&mut cursor, &empty[..empty_len]), Ok(0));
    assert_eq!(cursor.get_ref().as_slice(), b"> ");
}

#[cfg(feature = "std")]
#[test]
fn test_around_pos_past_initialized_data() {
    use zstd_safe::{InBuffer, OutBuffer};

    let end = zstd_safe::zstd_sys::ZSTD_EndDirective::ZSTD_e_end;
    let mut cctx = zstd_safe::CCtx::create();

    // A position past the `Vec`'s length: the gap must be zero-filled, not
    // left as whatever the allocator handed out.
    let mut buffer = Vec::with_capacity(256);
    let mut input = InBuffer::around(INPUT);
    let mut output = OutBuffer::around_pos(&mut buffer, 16);
    assert_eq!(output.as_slice(), &[0u8; 16]);
    while cctx.compress_stream2(&mut output, &mut input, end).unwrap() != 0 {}
    assert_eq!(&buffer[..16], &[0u8; 16]);
    assert!(buffer.len() > 16);

    // Same through a `Cursor` that itself sits past the `Vec`'s length.
    let mut buffer = Vec::with_capacity(256);
    let mut cursor = std::io::Cursor::new(&mut buffer);
    cursor.set_position(4);
    let mut input = InBuffer::around(INPUT);
    let mut output = OutBuffer::around_pos(&mut cursor, 8);
    assert_eq!(output.as_slice(), &[0u8; 8]);
    while cctx.compress_stream2(&mut output, &mut input, end).unwrap() != 0 {}
    assert_eq!(&buffer[..12], &[0u8; 12]);
    assert!(buffer.len() > 12);

    // A position inside the initialized part keeps the existing data.
    let mut buffer = std::vec![7u8; 4];
    buffer.reserve(256);
    let mut input = InBuffer::around(INPUT);
    let mut output = OutBuffer::around_pos(&mut buffer, 2);
    assert_eq!(output.as_slice(), &[7u8; 2]);
    while cctx.compress_stream2(&mut output, &mut input, end).unwrap() != 0 {}
    assert_eq!(&buffer[..2], &[7u8; 2]);
    assert!(buffer.len() > 2);
}

#[cfg(feature = "std")]
#[test]
fn test_simple_cycle() {
    let mut buffer = std::vec![0u8; 256];
    let written = zstd_safe::compress(&mut buffer, INPUT, 3).unwrap();
    let compressed = &buffer[..written];

    let mut buffer = std::vec![0u8; 256];
    let written = zstd_safe::decompress(&mut buffer, compressed).unwrap();
    let decompressed = &buffer[..written];

    assert_eq!(INPUT, decompressed);
}

#[test]
fn test_cctx_cycle() {
    let mut buffer = std::vec![0u8; 256];
    let mut cctx = zstd_safe::CCtx::default();
    let written = cctx.compress(&mut buffer[..], INPUT, 1).unwrap();
    let compressed = &buffer[..written];

    let mut dctx = zstd_safe::DCtx::default();
    let mut buffer = std::vec![0u8; 256];
    let written = dctx.decompress(&mut buffer[..], compressed).unwrap();
    let decompressed = &buffer[..written];

    assert_eq!(INPUT, decompressed);
}

#[test]
fn test_dictionary() {
    // Prepare some content to train the dictionary.
    let bytes = LONG_CONTENT.as_bytes();
    // Include the actual newline bytes, including CRLF on Windows and a
    // possibly unterminated final line.
    let line_sizes: Vec<usize> = LONG_CONTENT
        .split_inclusive('\n')
        .map(|line| line.len())
        .collect();

    // Train the dictionary
    let mut dict_buffer = std::vec![0u8; 100_000];
    let written =
        zstd_safe::train_from_buffer(&mut dict_buffer[..], bytes, &line_sizes)
            .unwrap();
    let dict_buffer = &dict_buffer[..written];

    // Create pre-hashed dictionaries for (de)compression
    let cdict = zstd_safe::create_cdict(dict_buffer, 3);
    let ddict = zstd_safe::create_ddict(dict_buffer);

    // Compress data
    let mut cctx = zstd_safe::CCtx::default();
    cctx.ref_cdict(&cdict).unwrap();

    let mut buffer = std::vec![0u8; 1024 * 1024];
    // First, try to compress without a dict
    let big_written = zstd_safe::compress(&mut buffer[..], bytes, 3).unwrap();

    let written = cctx
        .compress2(&mut buffer[..], bytes)
        .map_err(zstd_safe::get_error_name)
        .unwrap();

    assert!(big_written > written);
    let compressed = &buffer[..written];

    // Decompress data
    let mut dctx = zstd_safe::DCtx::default();
    dctx.ref_ddict(&ddict).unwrap();

    let mut buffer = std::vec![0u8; 1024 * 1024];
    let written = dctx
        .decompress(&mut buffer[..], compressed)
        .map_err(zstd_safe::get_error_name)
        .unwrap();
    let decompressed = &buffer[..written];

    // Profit!
    assert_eq!(bytes, decompressed);
}

#[test]
#[should_panic(expected = "sample sizes don't add up")]
fn test_train_from_buffer_overflowing_sizes() {
    // These sizes only add up to the buffer length by wrapping around.
    let samples = [0u8; 100];
    let sizes = [usize::MAX - 99, 200];
    let mut dict_buffer = std::vec![0u8; 1024];
    let _ =
        zstd_safe::train_from_buffer(&mut dict_buffer[..], &samples, &sizes);
}

#[test]
fn test_checksum() {
    let mut buffer = std::vec![0u8; 256];
    let mut cctx = zstd_safe::CCtx::default();
    cctx.set_parameter(zstd_safe::CParameter::ChecksumFlag(true))
        .unwrap();
    let written = cctx.compress2(&mut buffer[..], INPUT).unwrap();
    let compressed = &mut buffer[..written];

    let mut dctx = zstd_safe::DCtx::default();
    let mut buffer = std::vec![0u8; 1024*1024];
    let written = dctx
        .decompress(&mut buffer[..], compressed)
        .map_err(zstd_safe::get_error_name)
        .unwrap();
    let decompressed = &buffer[..written];

    assert_eq!(INPUT, decompressed);

    // Now try again with some corruption
    // TODO: Find a mutation that _wouldn't_ be detected without checksums.
    // (Most naive changes already trigger a "corrupt block" error.)
    if let Some(last) = compressed.last_mut() {
        *last = last.saturating_sub(1);
    }
    let err = dctx
        .decompress(&mut buffer[..], compressed)
        .map_err(zstd_safe::get_error_name)
        .err()
        .unwrap();
    // The error message will complain about the checksum.
    assert!(err.contains("checksum"));
}

#[cfg(all(feature = "experimental", feature = "std"))]
#[test]
fn test_upper_bound() {
    let mut buffer = std::vec![0u8; 256];

    assert!(zstd_safe::decompress_bound(&buffer).is_err());

    let written = zstd_safe::compress(&mut buffer, INPUT, 3).unwrap();
    let compressed = &buffer[..written];

    assert_eq!(
        zstd_safe::decompress_bound(compressed),
        Ok(INPUT.len() as u64)
    );
}

#[cfg(feature = "experimental")]
#[test]
fn test_stable_in_buffer_cannot_be_enabled() {
    use zstd_safe::CParameter::StableInBuffer;

    // Enabling it would let `end_stream` read from an `InBuffer` whose
    // borrow ended with the `compress_stream` call that received it.
    let mut cctx = zstd_safe::CCtx::create();
    let err = cctx
        .set_parameter(StableInBuffer(true))
        .map_err(zstd_safe::get_error_name)
        .unwrap_err();
    assert!(err.contains("Unsupported parameter"));

    // Asking for the default is still fine.
    cctx.set_parameter(StableInBuffer(false)).unwrap();
}

#[cfg(feature = "seekable")]
#[test]
fn test_seekable_cycle() {
    let seekable_archive = new_seekable_archive(INPUT);
    let mut seekable = crate::seekable::Seekable::create();
    seekable
        .init_buff(&seekable_archive)
        .map_err(zstd_safe::get_error_name)
        .unwrap();

    check_seekable_decompression(|dst, offset| {
        seekable.decompress(dst, offset)
    });

    // A `Vec` takes its length from the result
    #[cfg(feature = "std")]
    {
        let mut buffer = Vec::with_capacity(64);
        assert_eq!(seekable.decompress(&mut buffer, 4096), Ok(0));
        assert!(buffer.is_empty());
    }

    // Check that the archive can also be decompressed by a regular function
    let mut buffer = std::vec![0u8; 256];
    let written = zstd_safe::decompress(&mut buffer[..], &seekable_archive)
        .map_err(zstd_safe::get_error_name)
        .unwrap();
    let decompressed = &buffer[..written];
    assert_eq!(INPUT, decompressed);

    // Trigger FrameIndexTooLargeError
    let frame_index = seekable.num_frames() + 1;
    assert_eq!(
        seekable.frame_compressed_offset(frame_index).unwrap_err(),
        crate::seekable::FrameIndexTooLargeError
    );
}

#[cfg(feature = "seekable")]
#[test]
fn test_seekable_seek_table() {
    use crate::seekable::{FrameIndexTooLargeError, SeekTable, Seekable};

    let seekable_archive = new_seekable_archive(INPUT);
    let mut seekable = Seekable::create();

    // Assert that creating a SeekTable from an uninitialized seekable errors.
    // This led to segfaults with zstd versions prior v1.5.7
    assert!(SeekTable::try_from_seekable(&seekable).is_err());

    seekable
        .init_buff(&seekable_archive)
        .map_err(zstd_safe::get_error_name)
        .unwrap();

    // Try to create a seek table from the seekable
    let seek_table = { SeekTable::try_from_seekable(&seekable).unwrap() };

    // Seekable and seek table should return the same results
    assert_eq!(seekable.num_frames(), seek_table.num_frames());
    assert_eq!(
        seekable.frame_compressed_offset(2).unwrap(),
        seek_table.frame_compressed_offset(2).unwrap()
    );
    assert_eq!(
        seekable.frame_decompressed_offset(2).unwrap(),
        seek_table.frame_decompressed_offset(2).unwrap()
    );
    assert_eq!(
        seekable.frame_compressed_size(2).unwrap(),
        seek_table.frame_compressed_size(2).unwrap()
    );
    assert_eq!(
        seekable.frame_decompressed_size(2).unwrap(),
        seek_table.frame_decompressed_size(2).unwrap()
    );

    // zstd itself would read past the end of the seek table here.
    let last = seekable.num_frames();
    assert!(seekable.frame_decompressed_size(last).is_err());
    assert!(seek_table.frame_decompressed_size(last).is_err());
    assert!(seekable.frame_compressed_size(last).is_err());
    assert!(seek_table.frame_compressed_size(last).is_err());

    // Trigger FrameIndexTooLargeError
    let frame_index = seekable.num_frames() + 1;
    assert_eq!(
        seek_table.frame_compressed_offset(frame_index).unwrap_err(),
        FrameIndexTooLargeError
    );
}

/// A seekable has no seek table until it is successfully initialized, and zstd
/// dereferences a null pointer looking for it.
#[cfg(feature = "seekable")]
#[test]
fn test_seekable_uninitialized() {
    use crate::seekable::Seekable;

    fn check(seekable: &mut Seekable<'_>, error: &str) {
        let mut buffer = std::vec![0u8; 64];
        assert_eq!(
            seekable
                .decompress(&mut buffer[..], 0)
                .map_err(zstd_safe::get_error_name),
            Err(error)
        );
        assert_eq!(
            seekable
                .decompress_frame(&mut buffer[..], 0)
                .map_err(zstd_safe::get_error_name),
            Err(error)
        );
        assert_eq!(
            seekable
                .frame_decompressed_size(0)
                .map_err(zstd_safe::get_error_name),
            Err(error)
        );
        assert_eq!(seekable.offset_to_frame_index(0), 0);
    }

    let seekable_archive = new_seekable_archive(INPUT);
    let mut seekable = Seekable::create();
    check(&mut seekable, "Context should be init first");

    // A failed initialization does not make it usable.
    let error = seekable
        .init_buff(b"this is not a seekable archive")
        .map_err(zstd_safe::get_error_name)
        .unwrap_err();
    check(&mut seekable, error);

    // A successful one does...
    seekable
        .init_buff(&seekable_archive)
        .map_err(zstd_safe::get_error_name)
        .unwrap();
    check_seekable_decompression(|dst, offset| {
        seekable.decompress(dst, offset)
    });

    // ... until the next failure.
    let error = seekable
        .init_buff(b"this is not a seekable archive")
        .map_err(zstd_safe::get_error_name)
        .unwrap_err();
    check(&mut seekable, error);
}

#[cfg(all(feature = "std", feature = "seekable"))]
#[test]
fn test_seekable_advanced_cycle() {
    use crate::seekable::Seekable;
    use std::{boxed::Box, io::Cursor};

    // Wrap the archive in a cursor that implements Read and Seek,
    // a file would also work
    let seekable_archive = Cursor::new(new_seekable_archive(INPUT));
    let mut seekable = Seekable::create()
        .init_advanced(Box::new(seekable_archive))
        .map_err(zstd_safe::get_error_name)
        .unwrap();

    check_seekable_decompression(|dst, offset| {
        seekable.decompress(dst, offset)
    });
}

#[cfg(all(feature = "std", feature = "seekable"))]
#[test]
fn test_seekable_advanced_panic() {
    use crate::seekable::Seekable;
    use std::io::{Cursor, Read, Result, Seek, SeekFrom};
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::{boxed::Box, rc::Rc};

    // Panics on the first read or seek once `calls` of them were made.
    struct Panicking {
        inner: Cursor<Vec<u8>>,
        calls: usize,
        _alive: Rc<()>,
    }

    impl Panicking {
        fn call(&mut self, name: &'static str) {
            if self.calls == 0 {
                self.calls = usize::MAX;
                std::panic::panic_any(name);
            }
            self.calls -= 1;
        }
    }

    impl Read for Panicking {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
            self.call("read");
            self.inner.read(buf)
        }
    }

    impl Seek for Panicking {
        fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
            self.call("seek");
            self.inner.seek(pos)
        }
    }

    let alive = Rc::new(());
    let source = |calls| {
        Box::new(Panicking {
            inner: Cursor::new(new_seekable_archive(INPUT)),
            calls,
            _alive: alive.clone(),
        })
    };

    // Loading the seek table seeks, reads, seeks and reads.
    for (calls, name) in [(0, "seek"), (1, "read")] {
        let panic = catch_unwind(AssertUnwindSafe(|| {
            Seekable::create().init_advanced(source(calls)).map(|_| ())
        }))
        .unwrap_err();
        assert_eq!(panic.downcast_ref(), Some(&name));
        // The source was not leaked.
        assert_eq!(Rc::strong_count(&alive), 1);
    }

    // Then decompressing seeks and reads.
    for (calls, name) in [(4, "seek"), (5, "read")] {
        let mut seekable = Seekable::create()
            .init_advanced(source(calls))
            .map_err(zstd_safe::get_error_name)
            .unwrap();
        let mut buffer = [0u8; 64];
        let panic = catch_unwind(AssertUnwindSafe(|| {
            seekable.decompress_frame(&mut buffer[..], 1)
        }))
        .unwrap_err();
        assert_eq!(panic.downcast_ref(), Some(&name));

        // The panic is only reported once.
        check_seekable_decompression(|dst, offset| {
            seekable.decompress(dst, offset)
        });
    }
}

/// The `init*` family starts a new session, so it recovers a poisoned context.
#[cfg(all(feature = "std", feature = "experimental"))]
#[test]
#[allow(deprecated)]
fn test_init_using_dictionaries_clears_the_poison() {
    use crate::{CCtx, CDict, DCtx, DDict, InBuffer, OutBuffer};

    let dict = &LONG_CONTENT.as_bytes()[..1000];
    let mut buffer = std::vec![0u8; 1024];

    let cdict = CDict::create(dict, 3);
    let mut cctx = CCtx::create();
    let error = cctx.2.record(Err(1usize.wrapping_neg())).unwrap_err();
    let mut compressed = Vec::with_capacity(1024);
    assert_eq!(
        cctx.compress_stream(
            &mut OutBuffer::around(&mut compressed),
            &mut InBuffer::around(INPUT),
        ),
        Err(error)
    );
    cctx.init_using_cdict(&cdict).unwrap();
    cctx.compress_stream(
        &mut OutBuffer::around(&mut compressed),
        &mut InBuffer::around(INPUT),
    )
    .unwrap();

    let ddict = DDict::create(dict);
    let mut dctx = DCtx::create();
    let garbage = [0xFFu8; 64];
    dctx.decompress_stream(
        &mut OutBuffer::around(&mut buffer[..]),
        &mut InBuffer::around(&garbage[..]),
    )
    .unwrap_err();
    dctx.init_using_ddict(&ddict).unwrap();
    let mut decoded = Vec::with_capacity(INPUT.len());
    let mut frame = Vec::with_capacity(crate::compress_bound(INPUT.len()));
    CCtx::create()
        .compress_using_dict(&mut frame, INPUT, dict, 3)
        .unwrap();
    dctx.decompress_stream(
        &mut OutBuffer::around(&mut decoded),
        &mut InBuffer::around(&frame),
    )
    .unwrap();
    assert_eq!(decoded, INPUT);
}

#[cfg(feature = "seekable")]
fn new_seekable_archive(input: &[u8]) -> Vec<u8> {
    use crate::{seekable::SeekableCStream, InBuffer, OutBuffer};

    // Make sure the buffer is big enough
    // The buffer needs to be bigger as the uncompressed data here as the seekable archive has
    // more meta data than actual compressed data because the input is really small and we use
    // a max_frame_size of 64, which is way to small for real-world usages.
    let mut buffer = std::vec![0u8; 512];
    let mut cstream = SeekableCStream::create();
    cstream
        .init(3, true, 64)
        .map_err(zstd_safe::get_error_name)
        .unwrap();
    let mut in_buffer = InBuffer::around(input);
    let mut out_buffer = OutBuffer::around(&mut buffer[..]);

    // This could get stuck if the buffer is too small
    while in_buffer.pos() < in_buffer.src.len() {
        cstream
            .compress_stream(&mut out_buffer, &mut in_buffer)
            .map_err(zstd_safe::get_error_name)
            .unwrap();
    }

    // Make sure everything is flushed to out_buffer
    loop {
        if cstream
            .end_stream(&mut out_buffer)
            .map_err(zstd_safe::get_error_name)
            .unwrap()
            == 0
        {
            break;
        }
    }

    Vec::from(out_buffer.as_slice())
}

/// Nothing keeps the public `InBuffer::pos` within `src`, and
/// `ZSTD_seekable_compressStream()` would read from `src + pos` regardless.
#[cfg(feature = "seekable")]
#[test]
fn test_seekable_cstream_input_pos_out_of_bounds() {
    use crate::{seekable::SeekableCStream, InBuffer, OutBuffer};

    let mut buffer = std::vec![0u8; 512];
    let mut cstream = SeekableCStream::create();
    cstream
        .init(3, true, 64)
        .map_err(zstd_safe::get_error_name)
        .unwrap();
    let mut in_buffer = InBuffer {
        src: &INPUT[..16],
        pos: 32,
    };
    let mut out_buffer = OutBuffer::around(&mut buffer[..]);

    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cstream.compress_stream(&mut out_buffer, &mut in_buffer)
        }));
    assert!(result.is_err());

    // A full frame would have been written had the rest of `INPUT` been read.
    assert_eq!(out_buffer.pos(), 0);
}

#[cfg(feature = "seekable")]
fn check_seekable_decompression<F>(mut decompress: F)
where
    F: FnMut(&mut [u8], u64) -> zstd_safe::SafeResult,
{
    // Make the buffer as big as max_frame_size so it can hold a complete frame
    let mut buffer = std::vec![0u8; 64];
    // Decompress only the first frame
    let written = decompress(&mut buffer[..], 0)
        .map_err(zstd_safe::get_error_name)
        .unwrap();
    let decompressed = &buffer[..written];
    assert!(INPUT.starts_with(decompressed));
    assert_eq!(decompressed.len(), 64);

    // Make the buffer big enough to hold the complete input
    let mut buffer = std::vec![0u8; 256];
    // Decompress everything
    let written = decompress(&mut buffer[..], 0)
        .map_err(zstd_safe::get_error_name)
        .unwrap();
    let decompressed = &buffer[..written];
    assert_eq!(INPUT, decompressed);

    // Stop at the end of the data
    let end = INPUT.len() as u64;
    let written = decompress(&mut buffer[..], end - 10)
        .map_err(zstd_safe::get_error_name)
        .unwrap();
    assert_eq!(&INPUT[INPUT.len() - 10..], &buffer[..written]);

    // There is nothing to decompress from there on, no matter how far
    for offset in [end, end + 1, end + 4096, u64::MAX - 9, u64::MAX] {
        assert_eq!(decompress(&mut buffer[..], offset), Ok(0));
        assert_eq!(decompress(&mut buffer[..30], offset), Ok(0));
    }
}

/// An error can leave a context in a state zstd calls undefined, and says it is
/// UB to keep using. The context should refuse instead, until it is reset.
///
/// https://github.com/gyscos/zstd-rs/issues/315
#[cfg(feature = "std")]
#[test]
fn test_context_poisoned_by_an_error() {
    use zstd_safe::{DCtx, InBuffer, OutBuffer, ResetDirective};

    let mut dctx = DCtx::create();
    let mut buffer = std::vec![0u8; 1024];

    // Not a zstd frame: this fails, and may leave the context undefined.
    let garbage = [0xFFu8; 64];
    let first = {
        let mut input = InBuffer::around(&garbage[..]);
        let mut output = OutBuffer::around(&mut buffer[..]);
        dctx.decompress_stream(&mut output, &mut input).unwrap_err()
    };

    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(INPUT.len()));
    zstd_safe::compress(&mut compressed, INPUT, 3)
        .map_err(zstd_safe::get_error_name)
        .unwrap();

    // Even perfectly good input must not reach zstd now: the context has to be
    // reset first. The same error comes back, and nothing was consumed.
    let second = {
        let mut input = InBuffer::around(&compressed[..]);
        let mut output = OutBuffer::around(&mut buffer[..]);
        let err = dctx.decompress_stream(&mut output, &mut input).unwrap_err();
        assert_eq!(input.pos(), 0, "the poisoned context consumed input");
        assert_eq!(output.pos(), 0, "the poisoned context produced output");
        err
    };
    assert_eq!(
        zstd_safe::get_error_name(first),
        zstd_safe::get_error_name(second)
    );

    // Resetting the session makes it usable again.
    dctx.reset(ResetDirective::SessionOnly)
        .map_err(zstd_safe::get_error_name)
        .unwrap();

    let written = {
        let mut input = InBuffer::around(&compressed[..]);
        let mut output = OutBuffer::around(&mut buffer[..]);
        dctx.decompress_stream(&mut output, &mut input)
            .map_err(zstd_safe::get_error_name)
            .unwrap();
        output.pos()
    };
    assert_eq!(&buffer[..written], INPUT);
}

/// The legacy decoders report both buffers as fully processed when they fail,
/// although nothing was written. A failed call must not move the positions,
/// or a `Vec` ends up with a length covering uninitialized capacity.
#[cfg(all(feature = "std", feature = "legacy"))]
#[test]
fn test_failed_legacy_stream_reports_no_output() {
    use zstd_safe::{DCtx, InBuffer, OutBuffer};

    // zstd v0.7 magic, then a frame header descriptor with a reserved bit set.
    let corrupt_legacy_frame = [0x27u8, 0xB5, 0x2F, 0xFD, 0x08, 0x00];

    let mut dctx = DCtx::create();
    let mut buffer = Vec::with_capacity(1024);
    let mut input = InBuffer::around(&corrupt_legacy_frame[..]);
    let mut output = OutBuffer::around(&mut buffer);
    dctx.decompress_stream(&mut output, &mut input).unwrap_err();
    assert_eq!(output.pos(), 0, "a failed call reported output");
    assert_eq!(input.pos(), 0, "a failed call reported consumed input");
    assert_eq!(buffer.len(), 0);
}

/// The one-shot calls start a new job without resetting the stream stage in C,
/// so a stream they interrupted used to carry on afterwards - decoding against
/// the dictionary of the one-shot call, which may be long gone.
#[cfg(feature = "std")]
#[test]
fn test_one_shot_decompression_resets_the_stream() {
    use zstd_safe::{DCtx, DDict, InBuffer, OutBuffer, SafeResult};

    type OneShot = fn(&mut DCtx<'_>, &mut Vec<u8>, &[u8]) -> SafeResult;
    let one_shots: [OneShot; 3] = [
        |dctx, dst, src| dctx.decompress(dst, src),
        |dctx, dst, src| {
            dctx.decompress_using_dict(dst, src, &std::vec![b'd'; 1000])
        },
        |dctx, dst, src| {
            let ddict = DDict::create(&[b'd'; 1000]);
            dctx.decompress_using_ddict(dst, src, &ddict)
        },
    ];

    let long = LONG_CONTENT.as_bytes();
    let mut abandoned =
        Vec::with_capacity(zstd_safe::compress_bound(long.len()));
    zstd_safe::compress(&mut abandoned, long, 1).unwrap();
    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(INPUT.len()));
    zstd_safe::compress(&mut compressed, INPUT, 1).unwrap();

    for one_shot in one_shots {
        let mut dctx = DCtx::create();

        // A stream that is abandoned with output still buffered...
        let mut head = [0u8; 16];
        dctx.decompress_stream(
            &mut OutBuffer::around(&mut head[..]),
            &mut InBuffer::around(&abandoned),
        )
        .unwrap();

        // ... a one-shot job on the same context, with no room for output...
        one_shot(&mut dctx, &mut Vec::new(), &compressed).unwrap_err();

        // ... and a new stream, which must see nothing of either.
        let mut decoded = Vec::with_capacity(INPUT.len() + 1);
        let hint = dctx
            .decompress_stream(
                &mut OutBuffer::around(&mut decoded),
                &mut InBuffer::around(&compressed),
            )
            .map_err(zstd_safe::get_error_name)
            .unwrap();
        assert_eq!(hint, 0);
        assert_eq!(decoded, INPUT);
    }
}

/// Same on the compression side, whether or not the one-shot call succeeds.
#[cfg(feature = "std")]
#[test]
fn test_one_shot_compression_resets_the_stream() {
    use zstd_safe::{CCtx, CDict, InBuffer, OutBuffer, SafeResult};

    type OneShot = fn(&mut CCtx<'_>, &mut Vec<u8>, &[u8]) -> SafeResult;
    let one_shots: [OneShot; 4] = [
        |cctx, dst, src| cctx.compress(dst, src, 1),
        |cctx, dst, src| cctx.compress2(dst, src),
        |cctx, dst, src| {
            cctx.compress_using_dict(dst, src, &std::vec![b'd'; 1000], 1)
        },
        |cctx, dst, src| {
            let cdict = CDict::create(&[b'd'; 1000], 1);
            cctx.compress_using_cdict(dst, src, &cdict)
        },
    ];

    let long = LONG_CONTENT.as_bytes();
    let bound = zstd_safe::compress_bound(long.len());

    for (i, one_shot) in one_shots.iter().enumerate() {
        for capacity in [0, bound] {
            let mut cctx = CCtx::create();
            let mut compressed = Vec::with_capacity(bound);

            // A stream that is abandoned with input still buffered...
            cctx.compress_stream(
                &mut OutBuffer::around(&mut compressed),
                &mut InBuffer::around(INPUT),
            )
            .unwrap();
            compressed.clear();

            // ... a one-shot job on the same context...
            let res =
                one_shot(&mut cctx, &mut Vec::with_capacity(capacity), INPUT);
            assert_eq!(res.is_ok(), capacity != 0);

            // A failed `compress2` is left mid-frame (it may have worker
            // threads still running), so streaming waits for a reset.
            if i == 1 && res.is_err() {
                cctx.compress_stream(
                    &mut OutBuffer::around(&mut compressed),
                    &mut InBuffer::around(long),
                )
                .unwrap_err();
                assert!(compressed.is_empty());
                cctx.reset(zstd_safe::ResetDirective::SessionOnly).unwrap();
            }

            // ... and a new stream, which must see nothing of either.
            let mut output = OutBuffer::around(&mut compressed);
            cctx.compress_stream(&mut output, &mut InBuffer::around(long))
                .map_err(zstd_safe::get_error_name)
                .unwrap();
            let remaining = cctx
                .end_stream(&mut output)
                .map_err(zstd_safe::get_error_name)
                .unwrap();
            assert_eq!(remaining, 0);

            let mut decoded = Vec::with_capacity(long.len());
            zstd_safe::decompress(&mut decoded, &compressed)
                .map_err(zstd_safe::get_error_name)
                .unwrap();
            assert_eq!(decoded, long);
        }
    }
}

#[test]
fn test_poison_tracking() {
    use crate::Poison;

    let mut poison = Poison::default();
    assert!(poison.guard().is_ok());

    // Successes leave it alone.
    assert_eq!(poison.record(Ok(3)), Ok(3));
    assert!(poison.guard().is_ok());

    // A failure is remembered, and handed back to every later caller.
    assert_eq!(poison.record(Err(42)), Err(42));
    assert_eq!(poison.guard(), Err(42));
    // Nothing but a reset clears it - not even a later success.
    assert_eq!(poison.record(Ok(1)), Ok(1));
    assert_eq!(poison.guard(), Err(42));

    poison.clear();
    assert!(poison.guard().is_ok());
}

#[cfg(all(feature = "std", feature = "experimental", feature = "zstdmt"))]
#[test]
#[allow(deprecated)] // Exercises the deprecated block API.
fn test_multithreaded_context_refuses_block_api() {
    use crate::{CCtx, CParameter, InBuffer, OutBuffer, ResetDirective};

    let mut output = [0; 1024];
    let mut start_frame = |cctx: &mut CCtx<'_>| {
        cctx.compress_stream(
            &mut OutBuffer::around(&mut output[..]),
            &mut InBuffer::around(b"abc"),
        )
        .unwrap();
    };

    // Abandon a frame in its initial stage, with a workspace sized for it.
    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::CompressionLevel(19))
        .unwrap();
    start_frame(&mut cctx);
    cctx.reset(ResetDirective::SessionAndParameters).unwrap();

    // The workers' parameters now describe tables of other sizes, but the
    // context's own stage and workspace are still those of the first frame.
    cctx.set_parameter(CParameter::NbWorkers(1)).unwrap();
    start_frame(&mut cctx);
    let block = std::vec![0u8; cctx.get_block_size()];
    let mut compressed = Vec::with_capacity(block.len());
    assert!(unsafe { cctx.compress_block(&mut compressed, &block) }.is_err());
}

#[cfg(all(feature = "std", feature = "with-rust-allocator"))]
#[test]
fn test_custom_allocator_dictionaries_interoperate_with_c_dictionaries() {
    use crate::{zstd_sys, CCtx, CDict, DCtx};

    for dict_size in [512, 4096] {
        let dict: Vec<u8> = (0..=255u8).cycle().take(dict_size).collect();
        for level in [-3, 0, 3, 19] {
            let rust_dict = CDict::create(&dict, level);
            // This dictionary owns C-allocated storage and frees it through
            // its own callbacks, independently of the context's allocator.
            let c_dict = CDict::from_raw(unsafe {
                zstd_sys::ZSTD_createCDict(
                    crate::ptr_void(&dict),
                    dict.len(),
                    level,
                )
            })
            .unwrap();
            for input_size in [256, 256 * 1024] {
                let input: Vec<u8> =
                    dict.iter().copied().cycle().take(input_size).collect();
                let mut compressed =
                    Vec::with_capacity(crate::compress_bound(input_size));
                let mut decoded = Vec::with_capacity(input_size);
                for dictionary in [&rust_dict, &c_dict] {
                    CCtx::create()
                        .compress_using_cdict(
                            &mut compressed,
                            &input,
                            dictionary,
                        )
                        .unwrap();
                    DCtx::create()
                        .decompress_using_dict(
                            &mut decoded,
                            &compressed,
                            &dict,
                        )
                        .unwrap();
                    assert_eq!(decoded, input);
                }
            }
        }
    }
}

/// Compressible data, `len` bytes, different for each seed.
#[cfg(all(feature = "std", feature = "zstdmt"))]
fn words(len: usize, seed: u64) -> Vec<u8> {
    let words: [&[u8]; 6] =
        [b"lorem ", b"ipsum ", b"dolor ", b"sit ", b"amet ", b"elit "];
    let mut x = seed | 1;
    let mut data = Vec::with_capacity(len + 8);
    while data.len() < len {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        data.extend_from_slice(words[(x % 6) as usize]);
    }
    data.truncate(len);
    data
}

/// Starts a multithreaded frame with enough input for its jobs to still be
/// running on the worker threads when the call returns, against a 1 MiB
/// dictionary. Freeing that dictionary under them crashes a worker.
#[cfg(all(feature = "std", feature = "zstdmt"))]
fn start_multithreaded_frame(cctx: &mut zstd_safe::CCtx<'_>, input: &[u8]) {
    use zstd_safe::zstd_sys::ZSTD_EndDirective::ZSTD_e_flush;
    use zstd_safe::{CParameter, InBuffer, OutBuffer};

    cctx.set_parameter(CParameter::CompressionLevel(1)).unwrap();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    cctx.load_dictionary(&words(1 << 20, 1)).unwrap();
    // With no room for output, this returns as soon as the first job, the
    // one that reads the dictionary, was handed out.
    let mut empty: [u8; 0] = [];
    let mut output = OutBuffer::around(&mut empty[..]);
    let mut input = InBuffer::around(input);
    assert_ne!(
        cctx.compress_stream2(&mut output, &mut input, ZSTD_e_flush)
            .unwrap(),
        0
    );
}

/// Compresses `input` in one multithreaded frame with `dictionary`, and
/// checks that it decompresses back.
#[cfg(all(feature = "std", feature = "zstdmt"))]
fn check_round_trip(
    cctx: &mut zstd_safe::CCtx<'_>,
    input: &[u8],
    dictionary: &[u8],
) {
    use zstd_safe::CParameter;

    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    cctx.load_dictionary(dictionary).unwrap();
    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(input.len()));
    cctx.compress2(&mut compressed, input).unwrap();

    let mut dctx = zstd_safe::DCtx::create();
    dctx.load_dictionary(dictionary).unwrap();
    let mut decompressed = Vec::with_capacity(input.len());
    dctx.decompress(&mut decompressed, &compressed).unwrap();
    assert!(decompressed == input);
}

/// Every way of abandoning a multithreaded frame waits for its jobs, so the
/// dictionary can be replaced right after.
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_abandoned_multithreaded_frame() {
    use zstd_safe::{CCtx, ResetDirective};

    let input = words(24 << 20, 2);
    let small = words(1000, 3);
    let another = words(64 << 10, 4);
    type Abandon = fn(&mut CCtx<'_>, &[u8]);
    let ways: [(&str, Abandon); 10] = [
        ("reset(SessionOnly)", |cctx, _| {
            cctx.reset(ResetDirective::SessionOnly).unwrap();
        }),
        ("reset(SessionAndParameters)", |cctx, _| {
            cctx.reset(ResetDirective::SessionAndParameters).unwrap();
        }),
        ("wait_for_workers()", |cctx, _| {
            cctx.wait_for_workers().unwrap();
        }),
        ("init()", |cctx, _| {
            cctx.init(1).unwrap();
        }),
        ("compress_using_dict()", |cctx, small| {
            let mut buffer =
                Vec::with_capacity(zstd_safe::compress_bound(small.len()));
            cctx.compress_using_dict(&mut buffer, small, b"dictionary", 1)
                .unwrap();
        }),
        ("compress_using_cdict()", |cctx, small| {
            let cdict = zstd_safe::CDict::create(b"dictionary", 1);
            let mut buffer =
                Vec::with_capacity(zstd_safe::compress_bound(small.len()));
            cctx.compress_using_cdict(&mut buffer, small, &cdict)
                .unwrap();
        }),
        ("compress()", |cctx, small| {
            let mut buffer =
                Vec::with_capacity(zstd_safe::compress_bound(small.len()));
            cctx.compress(&mut buffer, small, 1).unwrap();
        }),
        // The way the scanner found: a short frame is compressed on the
        // calling thread, without zstd waiting for the workers.
        ("compress2() of a short source", |cctx, small| {
            let mut buffer =
                Vec::with_capacity(zstd_safe::compress_bound(small.len()));
            cctx.compress2(&mut buffer, small).unwrap();
        }),
        ("a failed compress2(), then a short one", |cctx, small| {
            cctx.reset(ResetDirective::SessionOnly).unwrap();
            let input = words(24 << 20, 5);
            cctx.compress2(&mut Vec::with_capacity(16), &input)
                .unwrap_err();
            let mut buffer =
                Vec::with_capacity(zstd_safe::compress_bound(small.len()));
            cctx.compress2(&mut buffer, small).unwrap();
        }),
        ("a short frame with ZSTD_e_end", |cctx, small| {
            use zstd_safe::zstd_sys::ZSTD_EndDirective::ZSTD_e_end;
            use zstd_safe::{InBuffer, OutBuffer};
            cctx.reset(ResetDirective::SessionOnly).unwrap();
            let mut buffer =
                Vec::with_capacity(zstd_safe::compress_bound(small.len()));
            let mut output = OutBuffer::around(&mut buffer);
            let mut input = InBuffer::around(small);
            assert_eq!(
                cctx.compress_stream2(&mut output, &mut input, ZSTD_e_end),
                Ok(0)
            );
        }),
    ];
    for (name, abandon) in ways {
        std::eprintln!("{name}");
        let mut cctx = CCtx::create();
        start_multithreaded_frame(&mut cctx, &input);
        abandon(&mut cctx, &small);
        check_round_trip(&mut cctx, &input, &another);
    }
}

/// Dropping a context in the middle of a multithreaded frame waits for its
/// jobs before freeing the dictionary. (A borrowed one was already safe with
/// the context's own workers, which freeing the context joins: see
/// `test_shared_thread_pool` for the case that was not.)
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_dropped_multithreaded_frame() {
    use zstd_safe::zstd_sys::ZSTD_EndDirective::ZSTD_e_flush;
    use zstd_safe::{CCtx, CDict, CParameter, InBuffer, OutBuffer};

    let input = words(24 << 20, 2);
    for _ in 0..3 {
        let mut cctx = CCtx::create();
        start_multithreaded_frame(&mut cctx, &input);
        drop(cctx);
    }

    let dictionary = words(1 << 20, 1);
    for _ in 0..3 {
        let cdict = CDict::create(&dictionary, 1);
        let mut cctx = CCtx::create();
        cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
        cctx.ref_cdict(&cdict).unwrap();
        let mut empty: [u8; 0] = [];
        let mut output = OutBuffer::around(&mut empty[..]);
        cctx.compress_stream2(
            &mut output,
            &mut InBuffer::around(&input),
            ZSTD_e_flush,
        )
        .unwrap();
        drop(cctx);
        drop(cdict);
    }
}

/// zstd up to 1.5.7 cannot wait for the job of an empty multithreaded
/// frame: no such frame is started.
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_empty_multithreaded_frame() {
    use zstd_safe::zstd_sys::ZSTD_EndDirective::{
        ZSTD_e_continue, ZSTD_e_end, ZSTD_e_flush,
    };
    use zstd_safe::{CCtx, CParameter, InBuffer, OutBuffer};

    let dictionary = words(1 << 20, 1);
    for _ in 0..3 {
        let mut cctx = CCtx::create();
        cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
        cctx.load_dictionary(&dictionary).unwrap();
        let mut empty: [u8; 0] = [];
        let mut output = OutBuffer::around(&mut empty[..]);
        let mut input = InBuffer::around(&[]);
        assert_eq!(
            cctx.compress_stream2(&mut output, &mut input, ZSTD_e_continue),
            Ok(0)
        );
        assert_eq!(
            cctx.compress_stream2(&mut output, &mut input, ZSTD_e_flush),
            Ok(0)
        );
        assert_eq!(cctx.flush_stream(&mut output), Ok(0));
        // The frame header does not fit: the frame stays in progress.
        assert_ne!(
            cctx.compress_stream2(&mut output, &mut input, ZSTD_e_end),
            Ok(0)
        );
        cctx.wait_for_workers().unwrap();
        cctx.load_dictionary(b"another").unwrap();
    }

    // It is still a valid, empty frame.
    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    let mut buffer = Vec::with_capacity(64);
    let mut output = OutBuffer::around(&mut buffer);
    let mut input = InBuffer::around(&[]);
    assert_eq!(
        cctx.compress_stream2(&mut output, &mut input, ZSTD_e_continue),
        Ok(0)
    );
    assert_eq!(cctx.end_stream(&mut output), Ok(0));
    let mut decompressed = Vec::with_capacity(16);
    assert_eq!(zstd_safe::decompress(&mut decompressed, &buffer), Ok(0));
}

/// zstd resets the session without waiting for the jobs when a frame it was
/// asked to end is given more input: that is refused first.
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_continue_after_end() {
    use zstd_safe::zstd_sys::ZSTD_EndDirective::{
        ZSTD_e_continue, ZSTD_e_end,
    };
    use zstd_safe::{CCtx, CParameter, InBuffer, OutBuffer};

    let data = words(24 << 20, 2);
    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(data.len()));
    let mut small = Vec::with_capacity(16);
    let mut input = InBuffer::around(&data);
    assert_ne!(
        cctx.compress_stream2(
            &mut OutBuffer::around(&mut small),
            &mut input,
            ZSTD_e_end
        ),
        Ok(0)
    );
    assert!(cctx
        .compress_stream2(
            &mut OutBuffer::around(&mut small),
            &mut input,
            ZSTD_e_continue
        )
        .is_err());
    // The frame can still be finished.
    compressed.extend_from_slice(&small);
    let written = compressed.len();
    let mut output = OutBuffer::around_pos(&mut compressed, written);
    while cctx
        .compress_stream2(&mut output, &mut input, ZSTD_e_end)
        .unwrap()
        != 0
    {}
    let mut decompressed = Vec::with_capacity(data.len());
    zstd_safe::decompress(&mut decompressed, &compressed).unwrap();
    assert!(decompressed == data);
}

/// After a job failed, zstd up to 1.5.7 can leave a cleared job behind that
/// the next multithreaded frame would hand to a worker: none is started.
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_failed_multithreaded_job() {
    use zstd_safe::zstd_sys::ZSTD_EndDirective::{
        ZSTD_e_continue, ZSTD_e_end,
    };
    use zstd_safe::{CCtx, CParameter, InBuffer, OutBuffer};

    let data = words(24 << 20, 2);
    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    // A job fails when it is given more than the pledged size.
    cctx.set_pledged_src_size(Some(1 << 20)).unwrap();
    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(data.len()));
    let mut output = OutBuffer::around(&mut compressed);
    let mut input = InBuffer::around(&data);
    // (Starting with `ZSTD_e_end` would pledge the size of the input.)
    let mut end_op = ZSTD_e_continue;
    loop {
        match cctx.compress_stream2(&mut output, &mut input, end_op) {
            Ok(0) if end_op == ZSTD_e_end => {
                panic!("the frame should have failed")
            }
            Ok(_) => end_op = ZSTD_e_end,
            Err(_) => break,
        }
    }

    let mut buffer = Vec::with_capacity(64);
    assert!(cctx.compress2(&mut buffer, b"hello").is_err());
    assert!(cctx.set_parameter(CParameter::NbWorkers(4)).is_err());
    cctx.reset(zstd_safe::ResetDirective::SessionOnly).unwrap();
    assert!(cctx.compress2(&mut buffer, b"hello").is_err());
    // Without workers, the context still works.
    cctx.set_parameter(CParameter::NbWorkers(0)).unwrap();
    cctx.compress2(&mut buffer, b"hello").unwrap();
}

/// A job can also fail while `compress2()` waits for the frame it abandons:
/// it does not start a multithreaded frame then either.
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_failed_multithreaded_job_found_by_compress2() {
    use zstd_safe::zstd_sys::ZSTD_EndDirective::ZSTD_e_continue;
    use zstd_safe::{CCtx, CParameter, InBuffer, OutBuffer};

    let data = words(24 << 20, 2);
    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    cctx.set_pledged_src_size(Some(1 << 20)).unwrap();
    // With no room for output, this returns while the first job, which
    // takes more than the pledged size, still runs.
    let mut empty: [u8; 0] = [];
    cctx.compress_stream2(
        &mut OutBuffer::around(&mut empty[..]),
        &mut InBuffer::around(&data),
        ZSTD_e_continue,
    )
    .unwrap();

    let mut buffer = Vec::with_capacity(64);
    assert!(cctx.compress2(&mut buffer, b"hello").is_err());
    assert!(cctx.compress2(&mut buffer, b"hello").is_err());
    cctx.set_parameter(CParameter::NbWorkers(0)).unwrap();
    cctx.compress2(&mut buffer, b"hello").unwrap();
}

/// An error before any job was handed out leaves the workers usable.
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_multithreaded_error_without_jobs() {
    use zstd_safe::{CCtx, CParameter};

    let data = words(24 << 20, 2);
    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    // A dictionary with zstd's magic number but nothing valid after it is
    // only rejected when a frame starts.
    let mut corrupted = 0xEC30A437u32.to_le_bytes().to_vec();
    corrupted.extend_from_slice(&[0xff; 64]);
    cctx.load_dictionary(&corrupted).unwrap();
    let mut buffer = Vec::with_capacity(zstd_safe::compress_bound(data.len()));
    assert!(cctx.compress2(&mut buffer, &data).is_err());

    cctx.load_dictionary(b"").unwrap();
    buffer.clear();
    cctx.compress2(&mut buffer, &data).unwrap();
}

/// Once a `ZSTD_e_end` call took all of its input, zstd made the frame's
/// last job: new input would wait forever behind it, so it is refused.
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_input_after_last_job() {
    use zstd_safe::zstd_sys::ZSTD_EndDirective::{
        ZSTD_e_continue, ZSTD_e_end, ZSTD_e_flush,
    };
    use zstd_safe::{CCtx, CParameter, InBuffer, OutBuffer};

    let data = words(12 << 20, 2);
    let more = words(1000, 3);

    // Ended with no input left.
    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(data.len()));
    let mut output = OutBuffer::around(&mut compressed);
    let mut input = InBuffer::around(&data);
    while input.pos() < data.len() {
        cctx.compress_stream2(&mut output, &mut input, ZSTD_e_continue)
            .unwrap();
    }
    let mut empty: [u8; 0] = [];
    assert_ne!(
        cctx.compress_stream2(
            &mut OutBuffer::around(&mut empty[..]),
            &mut InBuffer::around(&[]),
            ZSTD_e_end
        ),
        Ok(0)
    );
    for end_op in [ZSTD_e_end, ZSTD_e_flush] {
        assert!(cctx
            .compress_stream2(
                &mut output,
                &mut InBuffer::around(&more),
                end_op
            )
            .is_err());
    }
    // The frame can still be finished.
    while cctx.end_stream(&mut output).unwrap() != 0 {}
    let mut decompressed = Vec::with_capacity(data.len());
    zstd_safe::decompress(&mut decompressed, &compressed).unwrap();
    assert!(decompressed == data);

    // Ended once its input was all taken, by `end_stream()`.
    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    let mut input = InBuffer::around(&data);
    assert_ne!(
        cctx.compress_stream2(
            &mut OutBuffer::around(&mut empty[..]),
            &mut input,
            ZSTD_e_end
        ),
        Ok(0)
    );
    // (The rest of the input is dropped: the frame ends without it.)
    assert_ne!(
        cctx.end_stream(&mut OutBuffer::around(&mut empty[..])),
        Ok(0)
    );
    assert!(cctx
        .compress_stream2(
            &mut OutBuffer::around(&mut empty[..]),
            &mut InBuffer::around(&more),
            ZSTD_e_end
        )
        .is_err());
    cctx.reset(zstd_safe::ResetDirective::SessionOnly).unwrap();
    drop(cctx);
}

/// With a shared thread pool, zstd up to 1.5.7 never waits for the workers
/// when the context is freed.
#[cfg(all(feature = "std", feature = "experimental", feature = "zstdmt"))]
#[test]
fn test_shared_thread_pool() {
    use zstd_safe::{CCtx, ThreadPool};

    let input = words(24 << 20, 2);
    let pool = ThreadPool::new(2);
    for _ in 0..3 {
        let mut cctx = CCtx::create();
        cctx.ref_thread_pool(&pool).unwrap();
        start_multithreaded_frame(&mut cctx, &input);
        drop(cctx);
    }
    let mut cctx = CCtx::create();
    cctx.ref_thread_pool(&pool).unwrap();
    start_multithreaded_frame(&mut cctx, &input);
    cctx.reset(zstd_safe::ResetDirective::SessionOnly).unwrap();
    check_round_trip(&mut cctx, &input, &words(64 << 10, 4));
}

/// Waiting for the workers needs an output buffer of its own, which zstd
/// refuses with `StableOutBuffer`.
#[cfg(all(feature = "experimental", feature = "zstdmt"))]
#[test]
fn test_stable_output_with_workers() {
    use zstd_safe::{CCtx, CParameter};

    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    assert!(cctx
        .set_parameter(CParameter::StableOutBuffer(true))
        .is_err());
    cctx.set_parameter(CParameter::StableOutBuffer(false))
        .unwrap();

    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::StableOutBuffer(true))
        .unwrap();
    assert!(cctx.set_parameter(CParameter::NbWorkers(2)).is_err());
    cctx.set_parameter(CParameter::StableOutBuffer(false))
        .unwrap();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
}

/// One-shot decompressions only reset the session when a stream may be open.
/// Without one, the next stream must still see nothing of the one-shot call.
#[cfg(feature = "std")]
#[test]
fn test_one_shot_decompression_without_a_stream() {
    use zstd_safe::{DCtx, DDict, InBuffer, OutBuffer, SafeResult};

    type OneShot = fn(&mut DCtx<'_>, &mut Vec<u8>, &[u8]) -> SafeResult;
    let one_shots: [OneShot; 3] = [
        |dctx, dst, src| dctx.decompress(dst, src),
        |dctx, dst, src| {
            dctx.decompress_using_dict(dst, src, &std::vec![b'd'; 1000])
        },
        |dctx, dst, src| {
            let ddict = DDict::create(&[b'd'; 1000]);
            dctx.decompress_using_ddict(dst, src, &ddict)
        },
    ];

    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(INPUT.len()));
    zstd_safe::compress(&mut compressed, INPUT, 1).unwrap();

    for one_shot in one_shots {
        for capacity in [0, INPUT.len()] {
            let mut dctx = DCtx::create();
            let res = one_shot(
                &mut dctx,
                &mut Vec::with_capacity(capacity),
                &compressed,
            );
            assert_eq!(res.is_ok(), capacity != 0);

            let mut decoded = Vec::with_capacity(INPUT.len() + 1);
            let hint = dctx
                .decompress_stream(
                    &mut OutBuffer::around(&mut decoded),
                    &mut InBuffer::around(&compressed),
                )
                .map_err(zstd_safe::get_error_name)
                .unwrap();
            assert_eq!(hint, 0);
            assert_eq!(decoded, INPUT);

            // And a one-shot call after that stream ended still works.
            let mut out = Vec::with_capacity(INPUT.len());
            dctx.decompress(&mut out, &compressed).unwrap();
            assert_eq!(out, INPUT);
        }
    }
}

/// `is_error_kind` tells error kinds apart, and never matches a success.
#[test]
fn test_is_error_kind() {
    use zstd_safe::zstd_sys::ZSTD_ErrorCode::*;

    let mut small = [0u8; 4];
    let code = zstd_safe::compress(&mut small[..], INPUT, 3).unwrap_err();
    assert!(zstd_safe::is_error_kind(code, ZSTD_error_dstSize_tooSmall));
    assert!(!zstd_safe::is_error_kind(code, ZSTD_error_srcSize_wrong));

    let code = zstd_safe::decompress(&mut [0u8; 64][..], INPUT).unwrap_err();
    assert!(zstd_safe::is_error_kind(code, ZSTD_error_prefix_unknown));

    // `ZSTD_error_no_error` is 0, and no success value is an error.
    assert!(!zstd_safe::is_error_kind(0, ZSTD_error_no_error));
}

/// Compresses `input` as one streaming frame, in 128 KiB calls.
#[cfg(all(feature = "std", feature = "zstdmt"))]
fn stream_frame(cctx: &mut zstd_safe::CCtx<'_>, input: &[u8]) -> Vec<u8> {
    use zstd_safe::zstd_sys::ZSTD_EndDirective::{
        ZSTD_e_continue, ZSTD_e_end,
    };
    use zstd_safe::{InBuffer, OutBuffer};

    let mut compressed =
        Vec::with_capacity(zstd_safe::compress_bound(input.len()));
    let mut output = OutBuffer::around(&mut compressed);
    for chunk in input.chunks(128 << 10) {
        let mut src = InBuffer::around(chunk);
        while src.pos() < chunk.len() {
            cctx.compress_stream2(&mut output, &mut src, ZSTD_e_continue)
                .unwrap();
        }
    }
    let mut src = InBuffer::around(&[]);
    while cctx
        .compress_stream2(&mut output, &mut src, ZSTD_e_end)
        .unwrap()
        != 0
    {}
    compressed
}

/// Decompresses `compressed` with `prefix`, and checks it gives `input`.
#[cfg(all(feature = "std", feature = "zstdmt"))]
fn check_with_prefix(compressed: &[u8], prefix: &[u8], input: &[u8]) {
    let mut dctx = zstd_safe::DCtx::create();
    dctx.ref_prefix(prefix).unwrap();
    let mut decompressed = Vec::with_capacity(input.len());
    dctx.decompress(&mut decompressed, compressed).unwrap();
    assert!(decompressed == input);
}

/// A streaming frame that reads a borrowed prefix runs on the calling
/// thread: a leaked context would otherwise leave workers reading it after
/// the borrow ended. The next frame, without it, gets the workers back.
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_borrowed_prefix_frame_runs_on_calling_thread() {
    use zstd_safe::{CCtx, CParameter};

    let input = words(4 << 20, 2);
    let prefix = words(1 << 20, 3);
    let single = {
        let mut cctx = CCtx::create();
        cctx.set_parameter(CParameter::CompressionLevel(1)).unwrap();
        cctx.ref_prefix(&prefix).unwrap();
        stream_frame(&mut cctx, &input)
    };
    let multi = {
        let mut cctx = CCtx::create();
        cctx.set_parameter(CParameter::CompressionLevel(1)).unwrap();
        cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
        stream_frame(&mut cctx, &input)
    };

    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::CompressionLevel(1)).unwrap();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    assert_eq!(cctx.frame_workers(), 2);
    cctx.ref_prefix(&prefix).unwrap();
    assert_eq!(cctx.frame_workers(), 0);
    let compressed = stream_frame(&mut cctx, &input);
    assert!(compressed == single, "not compressed on the calling thread");
    check_with_prefix(&compressed, &prefix, &input);

    // The prefix was for that frame only.
    assert_eq!(cctx.frame_workers(), 2);
    assert!(stream_frame(&mut cctx, &input) == multi);
}

/// An owned prefix lives as long as the context, so its frames get the
/// workers, and a leaked context keeps it alive.
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_owned_prefix_uses_workers() {
    use self::std::sync::Arc;
    use zstd_safe::{CCtx, CParameter};

    let input = words(4 << 20, 2);
    let prefix: Arc<[u8]> = words(1 << 20, 3).into();
    let single = {
        let mut cctx = CCtx::create();
        cctx.set_parameter(CParameter::CompressionLevel(1)).unwrap();
        cctx.ref_prefix(&prefix).unwrap();
        stream_frame(&mut cctx, &input)
    };

    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::CompressionLevel(1)).unwrap();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    cctx.set_prefix(prefix.clone()).unwrap();
    assert_eq!(cctx.frame_workers(), 2);
    assert_eq!(Arc::strong_count(&prefix), 2);
    let compressed = stream_frame(&mut cctx, &input);
    assert!(compressed != single, "compressed on the calling thread");
    check_with_prefix(&compressed, &prefix, &input);

    // Leaked mid-frame, the context keeps the prefix alive for its workers.
    start_frame_with_prefix(&mut cctx, prefix.clone(), &input);
    self::std::mem::forget(cctx);
    assert_eq!(Arc::strong_count(&prefix), 2);

    // Dropped mid-frame, it waits for them, then lets the prefix go.
    let mut cctx = CCtx::create();
    start_frame_with_prefix(&mut cctx, prefix.clone(), &input);
    drop(cctx);
    assert_eq!(Arc::strong_count(&prefix), 2, "only the leaked one is left");
}

/// Starts a multithreaded frame against an owned `prefix`, returning while
/// its jobs still run.
#[cfg(all(feature = "std", feature = "zstdmt"))]
fn start_frame_with_prefix(
    cctx: &mut zstd_safe::CCtx<'_>,
    prefix: self::std::sync::Arc<[u8]>,
    input: &[u8],
) {
    use zstd_safe::zstd_sys::ZSTD_EndDirective::ZSTD_e_flush;
    use zstd_safe::{CParameter, InBuffer, OutBuffer};

    cctx.set_parameter(CParameter::CompressionLevel(1)).unwrap();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    cctx.set_prefix(prefix).unwrap();
    assert_eq!(cctx.frame_workers(), 2);
    let mut empty: [u8; 0] = [];
    let mut output = OutBuffer::around(&mut empty[..]);
    let mut input = InBuffer::around(input);
    assert_ne!(
        cctx.compress_stream2(&mut output, &mut input, ZSTD_e_flush)
            .unwrap(),
        0
    );
    assert!(cctx.3.pending && cctx.3.requested);
}

/// A context keeps its own handle to a dictionary: leaked, it keeps the
/// dictionary alive for its workers; dropped, it lets it go after them.
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_context_holds_its_dictionary() {
    use self::std::sync::Arc;
    use zstd_safe::zstd_sys::ZSTD_EndDirective::ZSTD_e_flush;
    use zstd_safe::{CCtx, CDict, CParameter, InBuffer, OutBuffer};

    let input = words(4 << 20, 2);
    let cdict = CDict::create(&words(1 << 20, 1), 1);
    let start = |cctx: &mut CCtx<'static>| {
        cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
        cctx.ref_cdict(&cdict).unwrap();
        assert_eq!(cctx.frame_workers(), 2);
        let mut empty: [u8; 0] = [];
        let mut output = OutBuffer::around(&mut empty[..]);
        let mut src = InBuffer::around(&input);
        assert_ne!(
            cctx.compress_stream2(&mut output, &mut src, ZSTD_e_flush)
                .unwrap(),
            0
        );
        assert!(cctx.3.pending && cctx.3.requested);
    };

    let mut leaked = CCtx::create();
    start(&mut leaked);
    assert_eq!(Arc::strong_count(&cdict.0), 2);
    self::std::mem::forget(leaked);

    let mut dropped = CCtx::create();
    start(&mut dropped);
    assert_eq!(Arc::strong_count(&cdict.0), 3);
    drop(dropped);
    assert_eq!(Arc::strong_count(&cdict.0), 2);

    // Replacing the dictionary lets the old one go.
    let mut cctx = CCtx::create();
    cctx.ref_cdict(&cdict).unwrap();
    assert_eq!(Arc::strong_count(&cdict.0), 3);
    cctx.disable_dictionary().unwrap();
    assert_eq!(Arc::strong_count(&cdict.0), 2);
    drop(cdict); // The leaked context still holds it.
}

/// `compress2()` returns once its frame is done, so it keeps the workers
/// with a borrowed prefix; when the destination is too small, it waits for
/// the frame before returning.
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_compress2_with_borrowed_prefix() {
    use zstd_safe::{CCtx, CParameter};

    let input = words(4 << 20, 2);
    let prefix = words(1 << 20, 3);
    let one_shot = |workers, owned: bool| {
        let mut cctx = CCtx::create();
        cctx.set_parameter(CParameter::CompressionLevel(1)).unwrap();
        cctx.set_parameter(CParameter::NbWorkers(workers)).unwrap();
        if owned {
            cctx.set_prefix(prefix.clone()).unwrap();
        } else {
            cctx.ref_prefix(&prefix).unwrap();
        }
        let mut compressed =
            Vec::with_capacity(zstd_safe::compress_bound(input.len()));
        cctx.compress2(&mut compressed, &input).unwrap();
        compressed
    };
    let borrowed = one_shot(2, false);
    assert!(borrowed == one_shot(2, true));
    assert!(borrowed != one_shot(0, false), "no workers");
    check_with_prefix(&borrowed, &prefix, &input);

    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::CompressionLevel(1)).unwrap();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    cctx.ref_prefix(&prefix).unwrap();
    let mut small = [0u8; 64];
    assert!(cctx.compress2(&mut small[..], &input).is_err());
    assert!(!cctx.3.pending, "returned with jobs running");
}
