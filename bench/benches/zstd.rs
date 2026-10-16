//! Benchmarks for the code the bindings add around zstd itself: buffer
//! plumbing, stream wrappers, context reuse, dictionaries, the seekable
//! format, zstdmt.
//!
//! zstd's own speed is not the point here (use zstd's `fullbench` for that),
//! so most inputs are small, where per-call overhead shows the most.
//!
//! Run with `cargo bench` from this directory; `ab.sh` compares revisions.
//! See README.md.

use std::io::{Cursor, Read, Write};

use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use zstd_safe::zstd_sys::ZSTD_EndDirective::{ZSTD_e_continue, ZSTD_e_end};
use zstd_safe::{CCtx, DCtx, InBuffer, OutBuffer};

const LEVEL: i32 = 3;
const SIZES: [usize; 4] = [64, 4 << 10, 128 << 10, 4 << 20];

/// Deterministic, text-like data that compresses about 3-4x.
fn data(len: usize) -> Vec<u8> {
    const WORDS: &[&str] = &[
        "the",
        "quick",
        "brown",
        "fox",
        "jumps",
        "over",
        "lazy",
        "dog",
        "zstd",
        "frame",
        "block",
        "window",
        "dictionary",
        "stream",
        "buffer",
        "context",
        "seekable",
        "worker",
        "entropy",
        "literal",
        "match",
        "offset",
        "sequence",
        "checksum",
        "\n",
        ", ",
        ". ",
        "0",
        "42",
        "1337",
    ];
    let mut x: u32 = 0x1234_5678;
    let mut out = Vec::with_capacity(len + 16);
    while out.len() < len {
        x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        out.extend_from_slice(
            WORDS[(x >> 16) as usize % WORDS.len()].as_bytes(),
        );
        out.push(b' ');
    }
    out.truncate(len);
    out
}

fn label(size: usize) -> String {
    match size {
        s if s >= 1 << 20 => format!("{}MiB", s >> 20),
        s if s >= 1 << 10 => format!("{}KiB", s >> 10),
        s => format!("{}B", s),
    }
}

/// One-shot APIs: `Vec` and `Cursor` destinations go through `WriteBuf`.
fn bulk(c: &mut Criterion) {
    let mut group = c.benchmark_group("bulk");
    for size in SIZES {
        let input = data(size);
        let compressed = zstd::bulk::compress(&input, LEVEL).unwrap();
        group.throughput(Throughput::Bytes(size as u64));

        let mut compressor = zstd::bulk::Compressor::new(LEVEL).unwrap();
        let mut out = Vec::with_capacity(zstd_safe::compress_bound(size));
        group.bench_function(BenchmarkId::new("compress", label(size)), |b| {
            b.iter(|| compressor.compress_to_buffer(&input, &mut out).unwrap())
        });

        let mut decompressor = zstd::bulk::Decompressor::new().unwrap();
        let mut out = Vec::with_capacity(size);
        group.bench_function(
            BenchmarkId::new("decompress", label(size)),
            |b| {
                b.iter(|| {
                    decompressor
                        .decompress_to_buffer(&compressed, &mut out)
                        .unwrap()
                })
            },
        );

        let mut cursor = Cursor::new(Vec::with_capacity(size + 8));
        group.bench_function(
            BenchmarkId::new("decompress_cursor", label(size)),
            |b| {
                b.iter(|| {
                    cursor.set_position(8);
                    decompressor
                        .decompress_to_buffer(&compressed, &mut cursor)
                        .unwrap()
                })
            },
        );
    }
    group.finish();
}

/// `Read`/`Write` wrappers (zio reader and writer).
fn stream(c: &mut Criterion) {
    let mut group = c.benchmark_group("stream");
    for size in SIZES {
        let input = data(size);
        let compressed = zstd::encode_all(&input[..], LEVEL).unwrap();
        group.throughput(Throughput::Bytes(size as u64));

        group.bench_function(
            BenchmarkId::new("encode_all", label(size)),
            |b| b.iter(|| zstd::encode_all(&input[..], LEVEL).unwrap()),
        );
        group.bench_function(
            BenchmarkId::new("decode_all", label(size)),
            |b| b.iter(|| zstd::decode_all(&compressed[..]).unwrap()),
        );

        // Small calls, where the wrappers' per-call work matters most.
        group.bench_function(
            BenchmarkId::new("read_decoder_256B_reads", label(size)),
            |b| {
                let mut buf = [0u8; 256];
                b.iter(|| {
                    let mut decoder =
                        zstd::stream::read::Decoder::new(&compressed[..])
                            .unwrap();
                    let mut total = 0;
                    loop {
                        match decoder.read(&mut buf).unwrap() {
                            0 => break total,
                            n => total += n,
                        }
                    }
                })
            },
        );
        group.bench_function(
            BenchmarkId::new("write_encoder_512B_writes", label(size)),
            |b| {
                b.iter(|| {
                    let mut encoder = zstd::stream::write::Encoder::new(
                        Vec::with_capacity(size),
                        LEVEL,
                    )
                    .unwrap();
                    for chunk in input.chunks(512) {
                        encoder.write_all(chunk).unwrap();
                    }
                    encoder.finish().unwrap()
                })
            },
        );
        group.bench_function(
            BenchmarkId::new("write_decoder_512B_writes", label(size)),
            |b| {
                b.iter(|| {
                    let mut decoder = zstd::stream::write::Decoder::new(
                        Vec::with_capacity(size),
                    )
                    .unwrap();
                    for chunk in compressed.chunks(512) {
                        decoder.write_all(chunk).unwrap();
                    }
                    decoder.flush().unwrap();
                    decoder.into_inner()
                })
            },
        );
    }
    group.finish();
}

/// Many small messages through one reused context (`with_context`).
fn with_context(c: &mut Criterion) {
    const MESSAGES: usize = 100;
    const MESSAGE: usize = 256;
    let messages: Vec<Vec<u8>> = (0..MESSAGES)
        .map(|i| {
            let mut m = data(MESSAGE + i);
            m.drain(..i);
            m
        })
        .collect();
    let compressed: Vec<Vec<u8>> = messages
        .iter()
        .map(|m| zstd::encode_all(&m[..], LEVEL).unwrap())
        .collect();

    let mut group = c.benchmark_group("with_context");
    group.throughput(Throughput::Bytes((MESSAGES * MESSAGE) as u64));

    let mut cctx = CCtx::create();
    cctx.set_parameter(zstd_safe::CParameter::CompressionLevel(LEVEL))
        .unwrap();
    group.bench_function("encode_100x256B", |b| {
        b.iter(|| {
            for m in &messages {
                let mut encoder = zstd::stream::write::Encoder::with_context(
                    Vec::with_capacity(MESSAGE),
                    &mut cctx,
                );
                encoder.write_all(m).unwrap();
                encoder.finish().unwrap();
            }
        })
    });

    let mut dctx = DCtx::create();
    group.bench_function("decode_100x256B", |b| {
        let mut out = Vec::with_capacity(2 * MESSAGE);
        b.iter(|| {
            for m in &compressed {
                out.clear();
                zstd::stream::read::Decoder::with_context(&m[..], &mut dctx)
                    .read_to_end(&mut out)
                    .unwrap();
            }
        })
    });
    group.finish();
}

/// zstd-safe streaming with tiny input chunks: one FFI call per 64 bytes, so
/// this is mostly the cost of the `InBuffer`/`OutBuffer` wrapping.
fn safe_stream(c: &mut Criterion) {
    const SIZE: usize = 128 << 10;
    const CHUNK: usize = 64;
    let input = data(SIZE);
    let compressed = zstd::bulk::compress(&input, LEVEL).unwrap();

    let mut group = c.benchmark_group("safe_stream");
    group.throughput(Throughput::Bytes(SIZE as u64));

    let mut cctx = CCtx::create();
    cctx.set_parameter(zstd_safe::CParameter::CompressionLevel(LEVEL))
        .unwrap();
    let mut out = Vec::with_capacity(zstd_safe::compress_bound(SIZE));
    group.bench_function("compress_stream2_64B_chunks", |b| {
        b.iter(|| {
            out.clear();
            let mut output = OutBuffer::around(&mut out);
            for chunk in input.chunks(CHUNK) {
                let mut src = InBuffer::around(chunk);
                while src.pos() < chunk.len() {
                    cctx.compress_stream2(
                        &mut output,
                        &mut src,
                        ZSTD_e_continue,
                    )
                    .unwrap();
                }
            }
            let mut src = InBuffer::around(&[]);
            while cctx
                .compress_stream2(&mut output, &mut src, ZSTD_e_end)
                .unwrap()
                != 0
            {}
            output.pos()
        })
    });

    let mut dctx = DCtx::create();
    let mut out = Vec::with_capacity(SIZE);
    group.bench_function("decompress_stream_64B_chunks", |b| {
        b.iter(|| {
            out.clear();
            let mut output = OutBuffer::around(&mut out);
            for chunk in compressed.chunks(CHUNK) {
                let mut src = InBuffer::around(chunk);
                while src.pos() < chunk.len() {
                    dctx.decompress_stream(&mut output, &mut src).unwrap();
                }
            }
            output.pos()
        })
    });
    group.finish();
}

/// Short reads at scattered offsets of a seekable archive.
fn seekable(c: &mut Criterion) {
    use zstd_safe::seekable::{Seekable, SeekableCStream};

    const SIZE: usize = 4 << 20;
    const FRAME: u32 = 64 << 10;
    const READS: usize = 64;
    const READ: usize = 256;
    let input = data(SIZE);

    let mut archive =
        Vec::with_capacity(zstd_safe::compress_bound(SIZE) + (1 << 16));
    {
        let mut cstream = SeekableCStream::create();
        cstream.init(LEVEL, false, FRAME).unwrap();
        let mut output = OutBuffer::around(&mut archive);
        let mut src = InBuffer::around(&input);
        while src.pos() < input.len() {
            cstream.compress_stream(&mut output, &mut src).unwrap();
        }
        while cstream.end_stream(&mut output).unwrap() != 0 {}
    }

    let offsets: Vec<u64> = (0..READS)
        .map(|i| (i as u64 * 2_654_435_761) % (SIZE - READ) as u64)
        .collect();

    let mut group = c.benchmark_group("seekable");
    group.throughput(Throughput::Bytes((READS * READ) as u64));
    let mut seekable = Seekable::create();
    seekable.init_buff(&archive).unwrap();
    group.bench_function("64_random_256B_reads", |b| {
        let mut buf = [0u8; READ];
        b.iter(|| {
            for &offset in &offsets {
                seekable.decompress(&mut buf[..], offset).unwrap();
            }
        })
    });
    group.bench_function("sequential_256B_reads_64KiB", |b| {
        let mut buf = [0u8; READ];
        b.iter(|| {
            for i in 0..(FRAME as usize / READ) {
                seekable
                    .decompress(&mut buf[..], (i * READ) as u64)
                    .unwrap();
            }
        })
    });
    group.finish();
}

/// Multithreaded compression, through both entry points.
fn zstdmt(c: &mut Criterion) {
    const SIZE: usize = 16 << 20;
    let input = data(SIZE);

    let mut group = c.benchmark_group("zstdmt");
    group.sample_size(10);
    group.throughput(Throughput::Bytes(SIZE as u64));
    group.bench_function("write_encoder_4_workers_16MiB", |b| {
        b.iter(|| {
            let mut encoder = zstd::stream::write::Encoder::new(
                Vec::with_capacity(SIZE / 2),
                LEVEL,
            )
            .unwrap();
            encoder.multithread(4).unwrap();
            for chunk in input.chunks(128 << 10) {
                encoder.write_all(chunk).unwrap();
            }
            encoder.finish().unwrap()
        })
    });

    let mut compressor = zstd::bulk::Compressor::new(LEVEL).unwrap();
    compressor.multithread(4).unwrap();
    let mut out = Vec::with_capacity(zstd_safe::compress_bound(SIZE));
    group.bench_function("bulk_4_workers_16MiB", |b| {
        b.iter(|| compressor.compress_to_buffer(&input, &mut out).unwrap())
    });
    group.finish();
}

/// Database-row-sized records, one per line, from the `*.jsonl` files in the
/// directory named by `ZSTD_BENCH_RECORDS` (skipped when it is unset).
///
/// Each record is compressed and decompressed on its own with a re-used
/// context, as a database or a message queue would: without a dictionary,
/// with a raw one, and with a prepared one. The dictionary is trained on
/// records outside of the measured slice.
fn records(c: &mut Criterion) {
    const MEASURED: usize = 2000;
    const DICT_SIZE: usize = 16 << 10;

    let dir = match std::env::var_os("ZSTD_BENCH_RECORDS") {
        Some(dir) => std::path::PathBuf::from(dir),
        None => return,
    };
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .expect("could not read ZSTD_BENCH_RECORDS")
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    files.sort();

    let mut group = c.benchmark_group("records");
    for path in files {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let content = std::fs::read(&path).unwrap();
        let all: Vec<&[u8]> = content
            .split(|&b| b == b'\n')
            .filter(|r| !r.is_empty())
            .collect();
        if all.len() < 2 * MEASURED {
            eprintln!("{}: only {} records, skipped", name, all.len());
            continue;
        }
        let (measured, training) = all.split_at(MEASURED);
        let bytes: usize = measured.iter().map(|r| r.len()).sum();
        let max = measured.iter().map(|r| r.len()).max().unwrap();
        let dict = zstd::dict::from_samples(training, DICT_SIZE).unwrap();
        let cdict = zstd::dict::EncoderDictionary::copy(&dict, LEVEL);
        let ddict = zstd::dict::DecoderDictionary::copy(&dict);

        let mut plain_c = zstd::bulk::Compressor::new(LEVEL).unwrap();
        let mut raw_c =
            zstd::bulk::Compressor::with_dictionary(LEVEL, &dict).unwrap();
        let mut prepared_c =
            zstd::bulk::Compressor::with_prepared_dictionary(&cdict).unwrap();
        let mut plain_d = zstd::bulk::Decompressor::new().unwrap();
        let mut raw_d =
            zstd::bulk::Decompressor::with_dictionary(&dict).unwrap();
        let mut prepared_d =
            zstd::bulk::Decompressor::with_prepared_dictionary(&ddict)
                .unwrap();

        let compress_all = |compressor: &mut zstd::bulk::Compressor<'_>| {
            measured
                .iter()
                .map(|r| compressor.compress(r).unwrap())
                .collect::<Vec<_>>()
        };
        let plain = compress_all(&mut plain_c);
        let with_dict = compress_all(&mut raw_c);
        let ratio = |frames: &[Vec<u8>]| {
            bytes as f64 / frames.iter().map(Vec::len).sum::<usize>() as f64
        };
        eprintln!(
            "records/{}: {} records, avg {} B, ratio {:.2} without and \
             {:.2} with a {} KiB dictionary",
            name,
            MEASURED,
            bytes / MEASURED,
            ratio(&plain),
            ratio(&with_dict),
            DICT_SIZE >> 10
        );

        group.throughput(Throughput::Bytes(bytes as u64));
        let mut out = Vec::with_capacity(zstd_safe::compress_bound(max));
        for (mode, compressor) in [
            ("plain", &mut plain_c),
            ("dict", &mut raw_c),
            ("prepared_dict", &mut prepared_c),
        ] {
            group.bench_function(
                BenchmarkId::new(format!("compress_{}", mode), &name),
                |b| {
                    b.iter(|| {
                        for r in measured {
                            out.clear();
                            compressor
                                .compress_to_buffer(r, &mut out)
                                .unwrap();
                        }
                    })
                },
            );
        }
        let mut out = Vec::with_capacity(max);
        for (mode, decompressor, frames) in [
            ("plain", &mut plain_d, &plain),
            ("dict", &mut raw_d, &with_dict),
            ("prepared_dict", &mut prepared_d, &with_dict),
        ] {
            group.bench_function(
                BenchmarkId::new(format!("decompress_{}", mode), &name),
                |b| {
                    b.iter(|| {
                        for frame in frames {
                            out.clear();
                            decompressor
                                .decompress_to_buffer(frame, &mut out)
                                .unwrap();
                        }
                    })
                },
            );
        }
    }
    group.finish();
}

criterion_group!(
    benches,
    bulk,
    stream,
    with_context,
    safe_stream,
    seekable,
    zstdmt,
    records
);
criterion_main!(benches);
