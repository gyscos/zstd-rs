//! An encoder's `run` returns an input-size hint that is never 0: the stream
//! wrappers read 0 as the end of a frame, and would start a new one.

use std::io::Write;
use zstd::stream::raw::{Encoder, Operation};

#[test]
fn encoder_run_hint_is_never_zero() {
    for workers in [0, 2] {
        let mut encoder = Encoder::new(3).unwrap();
        if workers != 0 {
            #[cfg(feature = "zstdmt")]
            encoder
                .set_parameter(zstd::zstd_safe::CParameter::NbWorkers(workers))
                .unwrap();
            #[cfg(not(feature = "zstdmt"))]
            continue;
        }
        let mut output = vec![0u8; 1 << 16];
        for input in [&b""[..], b"a", &[b'x'; 300_000][..]] {
            let status = encoder.run_on_buffers(input, &mut output).unwrap();
            assert_eq!(status.remaining, zstd::zstd_safe::CCtx::in_size());
        }
    }
}

#[test]
fn small_writes_make_one_frame() {
    let mut encoder =
        zstd::stream::write::Encoder::new(Vec::new(), 3).unwrap();
    for i in 0..1000u32 {
        encoder.write_all(&i.to_le_bytes()).unwrap();
    }
    let compressed = encoder.finish().unwrap();
    assert_eq!(
        zstd::zstd_safe::find_frame_compressed_size(&compressed),
        Ok(compressed.len())
    );
}
