#!/bin/sh

RUST_TARGET=1.64
bindgen="bindgen --no-layout-tests --blocklist-type=max_align_t --rustified-enum=.* --use-core --rust-target $RUST_TARGET"
experimental="-DZSTD_STATIC_LINKING_ONLY -DZDICT_STATIC_LINKING_ONLY"

run_bindgen()
{
        echo "/*
This file is auto-generated from the public API of the zstd library.
It is released under the same BSD license.

$(cat zstd/LICENSE)
*/"

    $bindgen $@
}

    for EXPERIMENTAL_ARG in "$experimental" ""; do
        if [ -z "$EXPERIMENTAL_ARG" ]; then EXPERIMENTAL=""; else EXPERIMENTAL="_experimental"; fi
        SUFFIX=${EXPERIMENTAL}
        filename=src/bindings${EXPERIMENTAL}.rs

        run_bindgen zstd.h --allowlist-type "ZSTD_.*" --allowlist-function "ZSTD_.*" --allowlist-var "ZSTD_.*" -- -Izstd/lib $EXPERIMENTAL_ARG > src/bindings_zstd${SUFFIX}.rs
        run_bindgen zdict.h --blocklist-type wchar_t -- -Izstd/lib $EXPERIMENTAL_ARG > src/bindings_zdict${SUFFIX}.rs
    done
