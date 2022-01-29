#!/bin/bash
declare -A varTypes
varTypes[CLEVEL_DEFAULT]=CompressionLevel
varTypes[CONTENTSIZE_UNKNOWN]=u64
varTypes[CONTENTSIZE_ERROR]=u64

header() {
    echo "// This file has been generated by $0"
}

fetch_constants() {
    rg 'pub const ZSTD_' $1 | while read pub const var vartype eq value; do
        vname=${var/:}
        newname=${vname/ZSTD_}
        vt=${varTypes[$newname]}
        if [ -z "$vt" ]
        then
            echo "pub const ${newname}: $vartype = zstd_sys::${vname};"
        else
            echo "pub const ${newname}: $vt = zstd_sys::${vname} as $vt;"
        fi
    done | sort
}

constants=$(fetch_constants zstd-sys/src/bindings_zstd.rs)
header > src/constants.rs
echo "$constants" >> src/constants.rs

(
    header
    comm -23 <(fetch_constants zstd-sys/src/bindings_zstd_experimental.rs) <(echo "$constants")
) > src/constants_experimental.rs