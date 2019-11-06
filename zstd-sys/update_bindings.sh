#!/bin/sh
bindgen zstd.h --ctypes-prefix ::libc --blacklist-type max_align_t --rustified-enum '.*' --use-core -o src/bindings.rs -- -Izstd/lib
bindgen zstd.h --ctypes-prefix libc --blacklist-type max_align_t --rustified-enum '.*' --use-core -o src/bindings_experimental.rs -- -Izstd/lib -DZSTD_STATIC_LINKING_ONLY
bindgen zstd.h --blacklist-type max_align_t --rustified-enum '.*' --use-core -o src/bindings_std.rs -- -Izstd/lib
bindgen zstd.h --blacklist-type max_align_t --rustified-enum '.*' --use-core -o src/bindings_std_experimental.rs -- -Izstd/lib -DZSTD_STATIC_LINKING_ONLY
