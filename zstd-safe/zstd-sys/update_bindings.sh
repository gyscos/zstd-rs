#!/bin/sh
bindgen="bindgen --blacklist-type=max_align_t --size_t-is-usize --rustified-enum=.* --use-core"
no_std="--ctypes-prefix libc"
experimental="-DZSTD_STATIC_LINKING_ONLY"

for NO_STD_ARG in "$no_std" ""; do
    for EXPERIMENTAL_ARG in "$experimental" ""; do
        if [ -z "$NO_STD_ARG" ]; then STD="_std"; else STD=""; fi
        if [ -z "$EXPERIMENTAL_ARG" ]; then EXPERIMENTAL=""; else EXPERIMENTAL="_experimental"; fi
        filename=src/bindings${STD}${EXPERIMENTAL}.rs
        (
            $bindgen zstd.h $NO_STD_ARG -- -Izstd/lib $EXPERIMENTAL_ARG
        ) > $filename
    done
done
