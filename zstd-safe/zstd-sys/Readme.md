# zstd-sys

This is the low-level auto-generated binding to the [zstd] library.
You probably don't want to use this library directly; instead, look at [zstd-rs] or [zstd-safe].

# Cargo features

Enabled by default: `legacy`, `zdict_builder`.

| Feature | Default | Description |
| --- | --- | --- |
| `legacy` | yes | Decode frames written by zstd versions older than 0.8. |
| `zdict_builder` | yes | Expose the dictionary builder. *Using* a dictionary always works. |
| `experimental` | | Expose zstd's experimental API, which has no stability guarantees. |
| `seekable` | | Expose the seekable format from zstd's `contrib`. |
| `zstdmt` | | Multi-threaded compression inside the C library. |
| `thin` | | Build a smaller C library, at some cost in speed and error reporting. |
| `debug` | | Enable zstd's debug logs. |
| `no_asm` | | Do not build the x86-64 assembly. It is only ever used on x86-64 anyway. |
| `fat-lto`, `thin-lto` | | Cross-language LTO. Only works if `clang` builds the C library. |
| `bindgen` | | Generate the bindings at build time rather than using the pre-generated ones. Needs `libclang`. |
| `pkg-config` | | Link against a system-installed libzstd instead of building the bundled source. |
| `vendored` | | Always build the bundled source, even when `pkg-config` is also enabled. |
| `cmake` | | Build the C library with zstd's own CMake files instead of the `cc` crate, which handles cross-compilation better. Needs `cmake`. |
| `no_wasm_shim` | | Do not build the bundled wasm shims, for a wasm toolchain that already provides a C standard library. |
| `non-cargo` | | Do not print cargo directives, for use from another build system. |
| `std` | | Deprecated, and does nothing: this crate never uses types from `std`. |

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

Then, running `cargo build` in this directory should
take care of building the C library and linking to it.

# Build-time bindgen

This library includes a pre-generated `bindings.rs` file.
You can also generate new bindings at build-time, using the `bindgen` feature:

```
cargo build --features bindgen
```

[zstd]: https://github.com/facebook/zstd
[zstd-rs]: https://github.com/gyscos/zstd-rs
[zstd-safe]: https://github.com/gyscos/zstd-rs/tree/main/zstd-safe
