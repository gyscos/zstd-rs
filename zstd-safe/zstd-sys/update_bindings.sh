#!/bin/sh

RUST_TARGET=1.64
# Keep the existing Copy/Clone API for opaque handles, as in build.rs.
opaque_handles='(ZSTD_(CCtx|DCtx|CDict|DDict|CCtx_params|seekable_CStream|seekable|seekTable|frameLog)_s|POOL_ctx_s)'
bindgen="bindgen --no-layout-tests --blocklist-type=max_align_t --rustified-enum=.* --use-core --rust-target $RUST_TARGET --with-derive-custom-struct=$opaque_handles=Copy,Clone"
experimental="-DZSTD_STATIC_LINKING_ONLY -DZDICT_STATIC_LINKING_ONLY -DZSTD_RUST_BINDINGS_EXPERIMENTAL"

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

        run_bindgen zstd.h \
            --allowlist-type "ZSTD_.*" \
            --allowlist-function "ZSTD_.*" \
            --allowlist-var "ZSTD_.*" \
            -- -Izstd/lib $EXPERIMENTAL_ARG > src/bindings_zstd${SUFFIX}.rs

        run_bindgen zdict.h \
            --allowlist-type "ZDICT_.*" \
            --allowlist-function "ZDICT_.*" \
            --allowlist-var "ZDICT_.*" \
            -- -Izstd/lib $EXPERIMENTAL_ARG > src/bindings_zdict${SUFFIX}.rs
    done

    # - ZSTD_seekable_initFile is blocked because it expects the c FILE type, rust files can directly be passed to init_advanced()
    run_bindgen zstd_seekable.h --allowlist-file ".*zstd_seekable.h$" --no-recursive-allowlist \
      --blocklist-function ZSTD_seekable_initFile \
      -- -Izstd/lib > src/bindings_zstd_seekable.rs
