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

#[cfg(all(feature = "std", feature = "experimental"))]
#[test]
#[allow(deprecated)] // Exercises the deprecated context-copying API.
fn test_cloning_poisoned_contexts_returns_the_original_error() {
    use crate::{CCtx, DCtx, InBuffer, OutBuffer};

    let mut dctx = DCtx::create();
    let mut output = [0; 1024];
    let error = dctx
        .decompress_stream(
            &mut OutBuffer::around(&mut output[..]),
            &mut InBuffer::around(&[0xff; 64]),
        )
        .unwrap_err();
    assert_eq!(dctx.try_clone().err(), Some(error));

    let mut cctx = CCtx::create();
    cctx.2.record(Err(error)).unwrap_err();
    assert_eq!(cctx.try_clone(None).err(), Some(error));
}

#[cfg(all(feature = "std", feature = "experimental"))]
#[test]
#[allow(deprecated)] // Exercises the deprecated context-copying API.
fn test_cloning_decompression_context_never_shares_what_it_owns() {
    use crate::{
        CCtx, DCtx, DDict, DParameter, InBuffer, OutBuffer, ResetDirective,
    };

    let dict = &LONG_CONTENT.as_bytes()[..1000];
    let ddict = DDict::create(dict);
    let mut compressed =
        Vec::with_capacity(crate::compress_bound(INPUT.len()));
    CCtx::create()
        .compress_using_dict(&mut compressed, INPUT, dict, 3)
        .unwrap();

    // The copy would free these along with the original.
    let mut dctx = DCtx::create();
    dctx.load_dictionary(dict).unwrap();
    assert!(dctx.try_clone().is_err());
    dctx.ref_prefix(dict).unwrap();
    assert!(dctx.try_clone().is_err());
    dctx.init_using_dict(dict).unwrap();
    assert!(dctx.try_clone().is_err());
    dctx.disable_dictionary().unwrap();
    assert!(dctx.try_clone().is_ok());

    // zstd keeps the table of dictionaries even after the parameter is reset.
    let mut dctx = DCtx::create();
    dctx.set_parameter(DParameter::RefMultipleDDicts(true))
        .unwrap();
    dctx.ref_ddict(&ddict).unwrap();
    assert!(dctx.try_clone().is_err());
    dctx.reset(ResetDirective::SessionAndParameters).unwrap();
    assert!(dctx.try_clone().is_err());

    // A referenced dictionary belongs to neither context.
    let mut dctx = DCtx::create();
    dctx.ref_ddict(&ddict).unwrap();
    // Part of a header leaves the original in the middle of a frame.
    dctx.decompress_stream(
        &mut OutBuffer::around(&mut [0; 8][..]),
        &mut InBuffer::around(&compressed[..3]),
    )
    .unwrap();
    let mut cloned = dctx.try_clone().unwrap();
    drop(dctx);

    let mut decoded = Vec::with_capacity(INPUT.len());
    let mut output = OutBuffer::around(&mut decoded);
    let hint = cloned
        .decompress_stream(&mut output, &mut InBuffer::around(&compressed))
        .unwrap();
    assert_eq!(hint, 0);
    assert_eq!(decoded, INPUT);

    // Once the streaming buffers exist, data has been decompressed.
    let mut dctx = DCtx::create();
    dctx.ref_ddict(&ddict).unwrap();
    dctx.decompress_stream(
        &mut OutBuffer::around(&mut [0; 8][..]),
        &mut InBuffer::around(&compressed),
    )
    .unwrap();
    assert!(dctx.try_clone().is_err());
}

#[cfg(all(feature = "std", feature = "experimental"))]
#[test]
#[allow(deprecated)] // Exercises the deprecated context-copying API.
fn test_cloning_initialized_compression_context() {
    use crate::{parse_code, zstd_sys, CCtx, DCtx};

    let cctx = CCtx::create();
    // The deprecated copy API requires the buffer-less initial stage,
    // which has no safe constructor in this wrapper.
    parse_code(unsafe { zstd_sys::ZSTD_compressBegin(cctx.0.as_ptr(), 3) })
        .unwrap();
    let mut cloned = cctx.try_clone(Some(INPUT.len() as u64)).unwrap();
    drop(cctx);

    let mut compressed =
        Vec::with_capacity(crate::compress_bound(INPUT.len()));
    cloned.compress(&mut compressed, INPUT, 3).unwrap();
    let mut decoded = Vec::with_capacity(INPUT.len());
    DCtx::create()
        .decompress(&mut decoded, &compressed)
        .unwrap();
    assert_eq!(decoded, INPUT);
}

#[cfg(all(feature = "std", feature = "experimental"))]
#[test]
#[allow(deprecated)] // Exercises the deprecated context-copying API.
fn test_cloning_decompression_context_keeps_streaming_parameters() {
    use crate::{DCtx, DParameter, InBuffer, OutBuffer};

    // A frame announcing a 128 MiB window, followed by a raw block of 1 byte.
    let frame = b"\x28\xb5\x2f\xfd\x00\x88\x08\x00\x00\x78";
    let mut output = [0; 16];

    // The window limit must survive the copy.
    let mut dctx = DCtx::create();
    dctx.set_parameter(DParameter::WindowLogMax(20)).unwrap();
    let mut cloned = dctx.try_clone().unwrap();
    assert_eq!(
        cloned
            .decompress_stream(
                &mut OutBuffer::around(&mut output[..]),
                &mut InBuffer::around(frame),
            )
            .map_err(zstd_safe::get_error_name),
        Err("Frame requires too much memory for decoding")
    );
    assert!(cloned.sizeof() < 1 << 20);

    // So must the output buffer mode: no window buffer is allocated when
    // the caller's output buffer is used as the window.
    let mut dctx = DCtx::create();
    dctx.set_parameter(DParameter::StableOutBuffer(true))
        .unwrap();
    let mut cloned = dctx.try_clone().unwrap();
    cloned
        .decompress_stream(
            &mut OutBuffer::around(&mut output[..]),
            &mut InBuffer::around(frame),
        )
        .unwrap();
    assert_eq!(output[0], b'x');
    assert!(cloned.sizeof() < 1 << 20);
}

#[cfg(all(feature = "std", feature = "experimental", feature = "zstdmt"))]
#[test]
#[allow(deprecated)] // Exercises the deprecated copy and block APIs.
fn test_multithreaded_context_refuses_copy_and_block_apis() {
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
    assert!(cctx.try_clone(Some(1000)).is_ok());
    cctx.reset(ResetDirective::SessionAndParameters).unwrap();

    // The workers' parameters now describe tables of other sizes, but the
    // context's own stage and workspace are still those of the first frame.
    cctx.set_parameter(CParameter::NbWorkers(1)).unwrap();
    start_frame(&mut cctx);
    assert!(cctx.try_clone(Some(1000)).is_err());
    let block = std::vec![0u8; cctx.get_block_size()];
    let mut compressed = Vec::with_capacity(block.len());
    assert!(unsafe { cctx.compress_block(&mut compressed, &block) }.is_err());

    // Resetting the requested parameters does not touch the applied ones.
    // (The frame was abandoned, so its workers have to be waited for first.)
    cctx.wait_for_workers().unwrap();
    cctx.reset(ResetDirective::SessionAndParameters).unwrap();
    assert!(cctx.try_clone(Some(1000)).is_err());

    // The other way round, the copy reads past the source's workspace.
    let mut cctx = CCtx::create();
    start_frame(&mut cctx);
    cctx.reset(ResetDirective::SessionAndParameters).unwrap();
    cctx.set_parameter(CParameter::CompressionLevel(19))
        .unwrap();
    cctx.set_parameter(CParameter::NbWorkers(1)).unwrap();
    start_frame(&mut cctx);
    assert!(cctx.try_clone(None).is_err());
}

#[cfg(all(feature = "std", feature = "with-rust-allocator"))]
#[test]
fn test_custom_allocator_dictionaries_interoperate_with_c_dictionaries() {
    use crate::{zstd_sys, CCtx, CDict, DCtx};
    use core::{marker::PhantomData, ptr::NonNull};

    for dict_size in [512, 4096] {
        let dict: Vec<u8> = (0..=255u8).cycle().take(dict_size).collect();
        for level in [-3, 0, 3, 19] {
            let rust_dict = CDict::create(&dict, level);
            // This dictionary owns C-allocated storage and frees it through
            // its own callbacks, independently of the context's allocator.
            let c_dict = CDict(
                NonNull::new(unsafe {
                    zstd_sys::ZSTD_createCDict(
                        crate::ptr_void(&dict),
                        dict.len(),
                        level,
                    )
                })
                .unwrap(),
                PhantomData,
            );
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

/// Worker threads may still be compressing the jobs of an abandoned
/// multithreaded frame, against the context's dictionary: nothing may unload
/// it, or change the number of workers, until they are done.
#[cfg(all(feature = "std", feature = "zstdmt"))]
#[test]
fn test_unfinished_multithreaded_frame() {
    use zstd_safe::zstd_sys::ZSTD_EndDirective::{
        ZSTD_e_continue, ZSTD_e_end,
    };
    use zstd_safe::{CCtx, CParameter, InBuffer, OutBuffer, ResetDirective};

    let long = LONG_CONTENT.as_bytes().repeat(64);
    let start_frame = |cctx: &mut CCtx<'_>| {
        let mut buffer = Vec::with_capacity(1 << 16);
        cctx.compress_stream2(
            &mut OutBuffer::around(&mut buffer),
            &mut InBuffer::around(&long),
            ZSTD_e_continue,
        )
        .unwrap();
    };

    let mut cctx = CCtx::create();
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    cctx.load_dictionary(b"some dictionary").unwrap();
    start_frame(&mut cctx);
    cctx.reset(ResetDirective::SessionOnly).unwrap();

    assert!(cctx.load_dictionary(b"another").is_err());
    assert!(cctx.disable_dictionary().is_err());
    assert!(cctx.ref_prefix(b"prefix").is_err());
    assert!(cctx.init(3).is_err());
    assert!(cctx.reset(ResetDirective::SessionAndParameters).is_err());
    assert!(cctx.set_parameter(CParameter::NbWorkers(4)).is_err());
    // Other parameters, and a session reset, are fine.
    cctx.set_parameter(CParameter::CompressionLevel(5)).unwrap();
    cctx.reset(ResetDirective::SessionOnly).unwrap();

    cctx.wait_for_workers().unwrap();
    cctx.load_dictionary(b"another").unwrap();
    cctx.set_parameter(CParameter::NbWorkers(4)).unwrap();
    cctx.reset(ResetDirective::SessionAndParameters).unwrap();

    // A frame that was finished leaves nothing to wait for.
    cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
    let mut buffer = Vec::with_capacity(zstd_safe::compress_bound(long.len()));
    let mut output = OutBuffer::around(&mut buffer);
    let mut input = InBuffer::around(&long);
    while cctx
        .compress_stream2(&mut output, &mut input, ZSTD_e_end)
        .unwrap()
        != 0
    {}
    cctx.load_dictionary(b"another").unwrap();

    // So does a successful one-shot compression; a failed one does not.
    cctx.compress2(&mut Vec::with_capacity(0), &long)
        .unwrap_err();
    assert!(cctx.load_dictionary(b"some dictionary").is_err());
    cctx.wait_for_workers().unwrap();
    cctx.compress2(&mut buffer, &long).unwrap();
    cctx.load_dictionary(b"some dictionary").unwrap();
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
