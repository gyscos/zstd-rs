//! The same work, done naively and done well, so you can see where the time
//! goes - and measure it on your own data.
//!
//! ```sh
//! cargo run --release --example throughput -- <file>
//! cargo run --release --features zstdmt --example throughput -- <file>
//! ```
//!
//! Build with `--release`. A debug build measures this crate's wrappers rather
//! than zstd, and is several times slower for reasons that have nothing to do
//! with how you call it.
//!
//! The C library is the reference point: `zstd -b3 <file>` benchmarks the same
//! level in memory, and its numbers are directly comparable to these.
//!
//! One caveat on reading the output: everything here runs in one process, so
//! the later measurements find an allocator that the earlier ones have already
//! warmed up. That flatters whichever case would otherwise be paying to fault
//! in fresh pages - usually the ones that grow a buffer. Comparisons within a
//! section are sound; if you want to pin down one number, measure it on its
//! own in a fresh process.
//!
//! The short version of what this tends to show:
//!
//! * Decompressing into a buffer of the right size is worth about a factor of
//!   two. The size is usually in the frame header, and `decode_all` now reads
//!   it for you - but a frame written by a streaming compressor does not carry
//!   one, so it cannot be sized for.
//! * For data already in memory, the one-shot `bulk` API beats the streaming
//!   one, and by a lot when the items are small.
//! * Threads are a large win when compressing something big, and a loss when
//!   compressing something small.
//! * The `Read`/`Write` wrappers themselves are not the problem: given a sized
//!   destination they land within about ten percent of `bulk`.

use clap::Parser;
use std::io::{Read, Write};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// File whose contents to use as the sample data.
    file: std::path::PathBuf,

    /// Compression level to measure.
    #[arg(short, long, default_value_t = 3)]
    level: i32,

    /// How many times to repeat each measurement.
    #[arg(short, long, default_value_t = 10)]
    iterations: usize,

    /// Worker threads to measure. Needs the `zstdmt` feature.
    #[arg(short, long, default_value_t = 8)]
    workers: u32,
}

/// Times `f` over `bytes` of input and reports the throughput.
fn measure<F: FnMut()>(
    label: &str,
    bytes: usize,
    iterations: usize,
    mut f: F,
) -> f64 {
    f(); // Warm up caches and any lazy allocation.

    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }

    let throughput =
        (bytes * iterations) as f64 / start.elapsed().as_secs_f64() / 1e6;
    println!("  {:<50}{:>9.1} MB/s", label, throughput);
    throughput
}

fn speedup(naive: f64, better: f64) {
    println!("  {:<50}{:>9.2}x\n", "", better / naive);
}

fn main() {
    let args = Args::parse();

    if cfg!(debug_assertions) {
        eprintln!(
            "warning: this is a debug build, the numbers below are not \
             meaningful. Re-run with --release.\n"
        );
    }

    let data =
        std::fs::read(&args.file).expect("could not read the input file");
    let level = args.level;
    let iterations = args.iterations;
    let bound = zstd::zstd_safe::compress_bound(data.len());

    println!(
        "{} bytes, level {}, {} iterations",
        data.len(),
        level,
        iterations
    );
    println!(
        "the C library, for comparison:  zstd -b{} {}\n",
        level,
        args.file.display()
    );

    // ---------------------------------------------------------------------
    println!("# Compressing a buffer you already hold");
    println!("Streaming a slice you already have in memory pays for buffering");
    println!("it does not need, and the frame it writes cannot record its");
    println!("decompressed size - which costs whoever reads it, later.\n");

    let naive = measure("encode_all", data.len(), iterations, || {
        zstd::encode_all(&data[..], level).unwrap();
    });
    let better = measure("bulk::compress", data.len(), iterations, || {
        zstd::bulk::compress(&data, level).unwrap();
    });
    speedup(naive, better);

    // ---------------------------------------------------------------------
    println!("# Decompressing a frame");
    println!("Whether the output can be sized up front depends on the frame:");
    println!("bulk::compress and the zstd CLI record the decompressed size,");
    println!("a streaming compressor cannot.\n");

    let sized = zstd::bulk::compress(&data, level).unwrap();
    let unsized_ = zstd::encode_all(&data[..], level).unwrap();
    println!(
        "  (from bulk::compress, header says {:?})",
        zstd::decompressed_size(&sized)
    );
    println!(
        "  (from encode_all,     header says {:?})",
        zstd::decompressed_size(&unsized_)
    );

    let naive = measure("decode_all, size not in the frame", data.len(), iterations, || {
        zstd::decode_all(&unsized_[..]).unwrap();
    });
    let better = measure("decode_all, size in the frame", data.len(), iterations, || {
        zstd::decode_all(&sized[..]).unwrap();
    });
    speedup(naive, better);

    println!("Sizing the buffer yourself gets the same thing, and a re-used");
    println!("context saves the last few percent.\n");

    measure("Decoder + read_to_end into a sized Vec", data.len(), iterations, || {
        let mut out = Vec::with_capacity(data.len());
        zstd::Decoder::new(&sized[..]).unwrap().read_to_end(&mut out).unwrap();
    });
    measure("write::Decoder + one write_all", data.len(), iterations, || {
        let mut out = Vec::with_capacity(data.len());
        let mut decoder = zstd::stream::write::Decoder::new(&mut out).unwrap();
        decoder.write_all(&sized).unwrap();
        decoder.flush().unwrap();
    });
    {
        let mut decompressor = zstd::bulk::Decompressor::new().unwrap();
        let mut out = Vec::with_capacity(data.len());
        measure("bulk::Decompressor, re-used", data.len(), iterations, || {
            out.clear();
            decompressor.decompress_to_buffer(&sized, &mut out).unwrap();
        });
    }
    println!();

    // ---------------------------------------------------------------------
    println!("# Many small items");
    println!("Per-item streaming setup dominates once the items are small.\n");

    let items: Vec<&[u8]> = data.chunks(4096).collect();
    let total: usize = items.iter().map(|i| i.len()).sum();

    let naive = measure("encode_all per item", total, iterations, || {
        for item in &items {
            zstd::encode_all(*item, level).unwrap();
        }
    });
    let better = {
        let mut compressor = zstd::bulk::Compressor::new(level).unwrap();
        let mut out = Vec::with_capacity(bound);
        measure("re-used bulk::Compressor per item", total, iterations, || {
            for item in &items {
                out.clear();
                compressor.compress_to_buffer(item, &mut out).unwrap();
            }
        })
    };
    speedup(naive, better);

    // ---------------------------------------------------------------------
    println!("# Compressing something large");
    println!("Workers need enough data to be worth their coordination: this is");
    println!("a win on a large input and a loss on a small one. Try it on both.\n");

    let naive = measure("one thread", data.len(), iterations, || {
        zstd::bulk::compress(&data, level).unwrap();
    });

    #[cfg(feature = "zstdmt")]
    {
        let mut compressor = zstd::bulk::Compressor::new(level).unwrap();
        compressor
            .set_parameter(zstd::zstd_safe::CParameter::NbWorkers(args.workers))
            .unwrap();
        let mut out = Vec::with_capacity(bound);
        let label = format!("{} workers", args.workers);
        let better = measure(&label, data.len(), iterations, || {
            out.clear();
            compressor.compress_to_buffer(&data, &mut out).unwrap();
        });
        speedup(naive, better);
    }
    #[cfg(not(feature = "zstdmt"))]
    {
        let _ = naive;
        println!("  (build with --features zstdmt to measure this)");
    }
}
