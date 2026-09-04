# zstd

[![crates.io](https://img.shields.io/crates/v/zstd.svg)](https://crates.io/crates/zstd)
[![BSD-3-Clause licensed](https://img.shields.io/badge/license-BSD--3--Clause-blue.svg)](./LICENSE)

[![Build on Linux](https://github.com/gyscos/zstd-rs/actions/workflows/linux.yml/badge.svg)](https://github.com/gyscos/zstd-rs/actions/workflows/linux.yml)
[![Build on Windows](https://github.com/gyscos/zstd-rs/actions/workflows/windows.yml/badge.svg)](https://github.com/gyscos/zstd-rs/actions/workflows/windows.yml)
[![Build on macOS](https://github.com/gyscos/zstd-rs/actions/workflows/macos.yml/badge.svg)](https://github.com/gyscos/zstd-rs/actions/workflows/macos.yml)
[![Build on wasm](https://github.com/gyscos/zstd-rs/actions/workflows/wasm.yml/badge.svg)](https://github.com/gyscos/zstd-rs/actions/workflows/wasm.yml)


This library is a rust binding for the [zstd compression library][zstd].

# [Documentation][doc]

## 1 - Add to `cargo.toml`

```bash
$ cargo add zstd
```

```toml
# Cargo.toml

[dependencies]
zstd = "0.14"
```

## 2 - Usage

This library provides `Read` and `Write` wrappers to handle (de)compression,
along with convenience functions to made common tasks easier.

For instance, `stream::copy_encode` and `stream::copy_decode` are easy-to-use
wrappers around `std::io::copy`. Check the [stream] example:

```rust
use std::io;

// This function use the convenient `copy_encode` method
fn compress(level: i32) {
    zstd::stream::copy_encode(io::stdin(), io::stdout(), level).unwrap();
}

// This function does the same thing, directly using an `Encoder`:
fn compress_manually(level: i32) {
    let mut encoder = zstd::stream::Encoder::new(io::stdout(), level).unwrap();
    io::copy(&mut io::stdin(), &mut encoder).unwrap();
    encoder.finish().unwrap();
}

fn decompress() {
    zstd::stream::copy_decode(io::stdin(), io::stdout()).unwrap();
}
```

# Asynchronous support

The [`async-compression`](https://github.com/Nemo157/async-compression/) crate
provides an async-ready integration of various compression algorithms,
including `zstd-rs`.

# Performance

This wrapper adds little over the C library. On the same data at level 3, a
re-used `bulk::Compressor` and `bulk::Decompressor` land within a few percent
of what `zstd -b3` reports for the same file. If you are seeing much less than
that, it is usually one of these:

* **A debug build.** Always measure with `--release`; a debug build times this
  crate's wrappers rather than zstd.
* **Growing a `Vec` rather than sizing it.** The convenience functions that
  return a `Vec` start from an empty one. On decompression, sizing the output
  up front - `bulk::decompress`, or `copy_decode` into a
  `Vec::with_capacity` - was worth roughly a factor of two in our measurements.
* **Streaming data that is already in memory.** For a slice you already hold,
  the one-shot `bulk` API avoids a copy through the streaming buffers. It is
  worth about ten percent on decompression, so it is a much smaller effect
  than sizing the output - the `Read`/`Write` wrappers are not the problem
  people usually assume they are.
* **Leaving compression single-threaded.** With the `zstdmt` feature and
  several workers, compression of a 33 MB input went about three times faster
  here. It is a loss on small inputs, where there is not enough data to keep
  the workers busy.

To measure all of these on your own data:

```
cargo run --release --features zstdmt --example throughput -- <file>
```

It prints the matching `zstd -b<level>` command so you can compare against the
C library directly.

# Cargo features

Enabled by default: `legacy`, `arrays`, `zdict_builder`.

| Feature | Default | Description |
| --- | --- | --- |
| `arrays` | yes | Use fixed-size arrays (`[u8; N]`) as output buffers. |
| `legacy` | yes | Decode frames written by zstd versions older than 0.8. |
| `zdict_builder` | yes | Train new dictionaries. *Using* a dictionary always works. |
| `experimental` | | Expose zstd's experimental API. It has no stability guarantees, and may change between zstd releases. |
| `zstdmt` | | Multi-threaded compression inside the C library. |
| `thin` | | Build a smaller C library, at some cost in speed and error reporting. |
| `debug` | | Enable zstd's debug logs. |
| `no_asm` | | Do not build the x86-64 assembly. |
| `fat-lto`, `thin-lto` | | Cross-language LTO. Only works if `clang` builds the C library. |
| `doc-cfg` | | Mark feature-gated items in the generated documentation. Needs a nightly compiler. |
| `wasm` | | Does nothing; kept so that existing dependants keep building. |

The following change how the C library is obtained or built:

| Feature | Default | Description |
| --- | --- | --- |
| `bindgen` | | Generate the bindings at build time rather than using the pre-generated ones. Needs `libclang`. |
| `pkg-config` | | Link against a system-installed libzstd instead of building the bundled source. |
| `vendored` | | Always build the bundled source, even when `pkg-config` is also enabled. Useful to force a static build from a dependent crate. |
| `cmake` | | Build the C library with zstd's own CMake files instead of the `cc` crate, which handles cross-compilation better. Needs `cmake`. |

The [zstd-sys readme] also covers the build-time environment variables, and
what to do if the linker complains about a hidden zstd symbol being
"referenced by DSO".

# Compile it yourself

`zstd` is included as a submodule. To get everything during your clone, use:

```
git clone https://github.com/gyscos/zstd-rs --recursive
```

Or, if you cloned it without the `--recursive` flag,
call this from inside the repository:

```
git submodule update --init
```

Then, running `cargo build` should take care
of building the C library and linking to it.

# Build-time bindgen

This library includes a pre-generated `bindings.rs` file.
You can also generate new bindings at build-time, using the `bindgen` feature:

```
cargo build --features bindgen
```

# TODO

* Benchmarks, optimizations, ...

# Disclaimer

This implementation is largely inspired by bozaro's [lz4-rs].

# License

* The zstd C library is under a dual BSD/GPLv2 license.
* This zstd-rs binding library is under a [BSD-3-Clause](LICENSE) license.

[zstd]: https://github.com/facebook/zstd
[lz4-rs]: https://github.com/bozaro/lz4-rs
[cargo-edit]: https://github.com/killercup/cargo-edit#cargo-add
[doc]: https://docs.rs/zstd
[stream]: examples/stream.rs
[submodule]: https://git-scm.com/book/en/v2/Git-Tools-Submodules
[zstd-sys readme]: zstd-safe/zstd-sys/Readme.md#symbol-visibility
