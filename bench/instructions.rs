//! Workloads for `instructions.sh`, which counts the instructions each one
//! executes (`perf stat -e instructions:u`) on several revisions.
//!
//! At a few hundred nanoseconds per call, timings move by a few percent with
//! code layout alone; instruction counts do not. These are the per-call
//! paths of the `safe_stream`, `with_context`, `records` and `frames` groups.
//!
//! Usage: `instructions WORKLOAD ROUNDS`, or `instructions list`.

use std::io::Write;

use zstd_safe::zstd_sys::ZSTD_EndDirective::{
    self, ZSTD_e_continue, ZSTD_e_end,
};
use zstd_safe::{CCtx, CParameter, InBuffer, OutBuffer, WriteBuf};

const WORKLOADS: [&str; 6] = [
    "safe_stream",
    "with_context",
    "prepared_dict",
    "frames_workers",
    "frames_workers_prefix",
    "frames_workers_alternating_prefix",
];

/// See `compress_stream` in benches/zstd.rs.
fn compress_stream<C: WriteBuf + ?Sized>(
    cctx: &mut CCtx<'_>,
    output: &mut OutBuffer<'_, C>,
    input: &mut InBuffer<'_>,
    end_op: ZSTD_EndDirective,
) -> usize {
    #[cfg(not(feature = "zstd-safe-8"))]
    return cctx.compress_stream(output, input, end_op).unwrap();
    #[cfg(feature = "zstd-safe-8")]
    return cctx.compress_stream2(output, input, end_op).unwrap();
}

/// Deterministic text, `len` bytes.
fn data(len: usize, seed: usize) -> Vec<u8> {
    const TEXT: &[u8] = b"the quick brown fox jumps over the lazy dog ";
    (0..len)
        .map(|i| TEXT[(i * 7 + seed) % TEXT.len()])
        .collect()
}

/// 100 messages of about 256 bytes, as in the `with_context` group.
fn messages() -> Vec<Vec<u8>> {
    (0..100).map(|i| data(256 + i, i)).collect()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let workload = args.next().expect("usage: instructions WORKLOAD ROUNDS");
    if workload == "list" {
        for w in WORKLOADS {
            println!("{w}");
        }
        return;
    }
    let rounds: usize = args.next().map_or(100, |r| r.parse().unwrap());
    let mut total = 0;
    match workload.as_str() {
        // 128 KiB in 64-byte calls, then the end of the frame.
        "safe_stream" => {
            let input = data(128 << 10, 0);
            let mut cctx = CCtx::create();
            cctx.set_parameter(CParameter::CompressionLevel(3)).unwrap();
            let mut out =
                Vec::with_capacity(zstd_safe::compress_bound(input.len()));
            for _ in 0..rounds {
                out.clear();
                let mut output = OutBuffer::around(&mut out);
                for chunk in input.chunks(64) {
                    let mut src = InBuffer::around(chunk);
                    while src.pos() < chunk.len() {
                        compress_stream(
                            &mut cctx,
                            &mut output,
                            &mut src,
                            ZSTD_e_continue,
                        );
                    }
                }
                let mut src = InBuffer::around(&[]);
                while compress_stream(
                    &mut cctx,
                    &mut output,
                    &mut src,
                    ZSTD_e_end,
                ) != 0
                {}
                total += output.pos();
            }
        }
        // One write encoder per message, on a re-used context.
        "with_context" => {
            let messages = messages();
            let mut cctx = CCtx::create();
            cctx.set_parameter(CParameter::CompressionLevel(3)).unwrap();
            for _ in 0..rounds {
                for m in &messages {
                    let mut encoder =
                        zstd::stream::write::Encoder::with_context(
                            Vec::with_capacity(512),
                            &mut cctx,
                        );
                    encoder.write_all(m).unwrap();
                    total += encoder.finish().unwrap().len();
                }
            }
        }
        // A bulk compressor with a prepared dictionary, one call per message.
        "prepared_dict" => {
            let messages = messages();
            let dictionary =
                zstd::dict::EncoderDictionary::copy(&data(16 << 10, 3), 3);
            let mut compressor =
                zstd::bulk::Compressor::with_prepared_dictionary(&dictionary)
                    .unwrap();
            let mut out = Vec::with_capacity(4096);
            for _ in 0..rounds {
                for m in &messages {
                    total +=
                        compressor.compress_to_buffer(m, &mut out).unwrap();
                }
            }
        }
        // As the `frames` group: one frame per message, with `NbWorkers(2)`,
        // and a borrowed 1 KiB prefix on none, all or every other frame.
        frames if frames.starts_with("frames_workers") => {
            let with_prefix = |i: usize| match frames {
                "frames_workers" => false,
                "frames_workers_prefix" => true,
                _ => i.is_multiple_of(2),
            };
            let messages = messages();
            let prefix = data(1 << 10, 5);
            let mut cctx = CCtx::create();
            cctx.set_parameter(CParameter::CompressionLevel(3)).unwrap();
            cctx.set_parameter(CParameter::NbWorkers(2)).unwrap();
            let mut out = vec![0u8; 4096];
            for _ in 0..rounds {
                for (i, m) in messages.iter().enumerate() {
                    if with_prefix(i) {
                        cctx.ref_prefix(&prefix).unwrap();
                    }
                    let mut output = OutBuffer::around(&mut out[..]);
                    let mut src = InBuffer::around(m);
                    while compress_stream(
                        &mut cctx,
                        &mut output,
                        &mut src,
                        ZSTD_e_end,
                    ) != 0
                    {}
                    total += output.pos();
                }
            }
        }
        other => panic!("unknown workload {other}, see `instructions list`"),
    }
    // Keeps the work from being optimized away.
    std::hint::black_box(total);
}
