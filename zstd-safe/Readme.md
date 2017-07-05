# zstd-safe

This is a thin, safe abstraction built on top of the bindings from `zstd-sys`.

It is close to a 1-for-1 mapping to the C functions, but uses rust types like slices instead of pointers and lengths.

For a more comfortable higher-level library, see [zstd-rs].

[zstd-rs]: https://github.com/gyscos/zstd-rs
