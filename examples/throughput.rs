//! Measures the throughput of the different ways this crate can compress and
//! decompress the same data.
//!
//! The C library is the reference point: `zstd -b3 <file>` benchmarks the same
//! level in memory, so its numbers are directly comparable to the ones printed
//! here.
//!
//! ```sh
//! cargo run --release --example throughput -- <file>
//! cargo run --release --features zstdmt --example throughput -- <file>
//! ```
//!
//! Build with `--release`. A debug build measures this crate's wrappers rather
//! than zstd, and will be several times slower for reasons that have nothing
//! to do with how you call it.
//!
//! Things this tends to show, on top of whatever your own data does:
//!
//! * Sizing the output buffer is the one that matters. The convenience
//!   functions that return a `Vec` start from an empty one and grow it as they
//!   go, which on decompression can cost about half the throughput - all of it
//!   in reallocation, none of it in zstd.
//! * The `Read`/`Write` wrappers are not the problem. Once the destination is
//!   sized, `Encoder` matches the one-shot `bulk` API for compression and comes
//!   within about ten percent of it for decompression, whether they are driven
//!   by `io::copy`, a single `write_all`, or `read_to_end`.
//! * `zstdmt` with several workers is a large win on big inputs and a loss on
//!   small ones - there has to be enough data to fill the workers.

use clap::Parser;
use std::io::{Read, Write};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// File whose contents to compress.
    file: std::path::PathBuf,

    /// Compression level to measure.
    #[arg(short, long, default_value_t = 3)]
    level: i32,

    /// How many times to repeat each measurement.
    #[arg(short, long, default_value_t = 10)]
    iterations: usize,

    /// Number of worker threads to measure. Needs the `zstdmt` feature.
    #[arg(short, long, default_value_t = 8)]
    workers: u32,
}

/// Runs `f` a few times and prints the throughput over `bytes` of input.
fn bench<F: FnMut()>(name: &str, bytes: usize, iterations: usize, mut f: F) {
    f(); // Warm up caches and any lazy allocation.

    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let seconds = start.elapsed().as_secs_f64();

    let throughput = (bytes * iterations) as f64 / seconds / 1e6;
    println!("{:<52}{:>9.1} MB/s", name, throughput);
}

fn main() {
    let args = Args::parse();

    if cfg!(debug_assertions) {
        eprintln!(
            "warning: this is a debug build, the numbers below are not meaningful.\n\
             Re-run with --release.\n"
        );
    }

    let data = std::fs::read(&args.file).expect("could not read the input file");
    let level = args.level;
    let iterations = args.iterations;
    let bound = zstd::zstd_safe::compress_bound(data.len());

    println!(
        "{} bytes, level {}, {} iterations",
        data.len(),
        level,
        iterations
    );
    println!("compare against the C library with: zstd -b{} {}\n", level, args.file.display());

    println!("-- compression --");

    bench("encode_all (grows a Vec)", data.len(), iterations, || {
        zstd::encode_all(&data[..], level).unwrap();
    });

    bench("copy_encode into a sized Vec", data.len(), iterations, || {
        let mut out = Vec::with_capacity(bound);
        zstd::stream::copy_encode(&data[..], &mut out, level).unwrap();
    });

    bench("Encoder + one write_all", data.len(), iterations, || {
        let mut out = Vec::with_capacity(bound);
        let mut encoder = zstd::Encoder::new(&mut out, level).unwrap();
        encoder.write_all(&data).unwrap();
        encoder.finish().unwrap();
    });

    bench("read::Encoder + read_to_end into a sized Vec", data.len(), iterations, || {
        let mut out = Vec::with_capacity(bound);
        let mut encoder =
            zstd::stream::read::Encoder::new(&data[..], level).unwrap();
        encoder.read_to_end(&mut out).unwrap();
    });

    bench("bulk::compress (new context per call)", data.len(), iterations, || {
        zstd::bulk::compress(&data, level).unwrap();
    });

    {
        let mut compressor = zstd::bulk::Compressor::new(level).unwrap();
        let mut out = Vec::with_capacity(bound);
        bench("bulk::Compressor, re-used, into a sized Vec", data.len(), iterations, || {
            out.clear();
            compressor.compress_to_buffer(&data, &mut out).unwrap();
        });
    }

    #[cfg(feature = "zstdmt")]
    {
        let mut compressor = zstd::bulk::Compressor::new(level).unwrap();
        compressor
            .set_parameter(zstd::zstd_safe::CParameter::NbWorkers(args.workers))
            .unwrap();
        let mut out = Vec::with_capacity(bound);
        let name = format!("... with {} workers (zstdmt)", args.workers);
        bench(&name, data.len(), iterations, || {
            out.clear();
            compressor.compress_to_buffer(&data, &mut out).unwrap();
        });
    }
    #[cfg(not(feature = "zstdmt"))]
    println!("(build with --features zstdmt to measure worker threads)");

    println!("\n-- decompression --");

    let compressed = zstd::bulk::compress(&data, level).unwrap();
    println!(
        "(compressed to {} bytes, ratio {:.2})",
        compressed.len(),
        data.len() as f64 / compressed.len() as f64
    );

    bench("decode_all (grows a Vec)", data.len(), iterations, || {
        zstd::decode_all(&compressed[..]).unwrap();
    });

    bench("copy_decode into a sized Vec", data.len(), iterations, || {
        let mut out = Vec::with_capacity(data.len());
        zstd::stream::copy_decode(&compressed[..], &mut out).unwrap();
    });

    bench("Decoder + read_to_end into a sized Vec", data.len(), iterations, || {
        let mut out = Vec::with_capacity(data.len());
        let mut decoder = zstd::Decoder::new(&compressed[..]).unwrap();
        decoder.read_to_end(&mut out).unwrap();
    });

    bench("write::Decoder + one write_all", data.len(), iterations, || {
        let mut out = Vec::with_capacity(data.len());
        let mut decoder = zstd::stream::write::Decoder::new(&mut out).unwrap();
        decoder.write_all(&compressed).unwrap();
        decoder.flush().unwrap();
    });

    bench("bulk::decompress (new context per call)", data.len(), iterations, || {
        zstd::bulk::decompress(&compressed, data.len()).unwrap();
    });

    {
        let mut decompressor = zstd::bulk::Decompressor::new().unwrap();
        let mut out = Vec::with_capacity(data.len());
        bench("bulk::Decompressor, re-used, into a sized Vec", data.len(), iterations, || {
            out.clear();
            decompressor.decompress_to_buffer(&compressed, &mut out).unwrap();
        });
    }
}
