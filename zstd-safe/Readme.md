# zstd-safe

This is a thin, safe abstraction built on top of the bindings from `zstd-sys`.

It is close to a 1-for-1 mapping to the C functions, but uses rust types like slices instead of pointers and lengths.
