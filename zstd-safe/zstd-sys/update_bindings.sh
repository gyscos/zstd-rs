#!/bin/sh
bindgen="bindgen --blacklist-type=max_align_t --size_t-is-usize --rustified-enum=.* --use-core"

$bindgen zstd.h --ctypes-prefix ::libc -o src/bindings.rs -- -Izstd/lib
$bindgen zstd.h --ctypes-prefix libc -o src/bindings_experimental.rs -- -Izstd/lib -DZSTD_STATIC_LINKING_ONLY
$bindgen zstd.h -o src/bindings_std.rs -- -Izstd/lib
$bindgen zstd.h -o src/bindings_std_experimental.rs -- -Izstd/lib -DZSTD_STATIC_LINKING_ONLY
