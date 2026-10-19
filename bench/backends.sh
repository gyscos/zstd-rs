#!/bin/sh
# Compares zstd's backends on this machine, at one revision:
#
#   - the bundled C library (the reference),
#   - the same C library, allocating through Rust's global allocator
#     (`zstd/with-rust-allocator`),
#   - libzstd-rs-sys, the Rust port (`zstd/rust-backend`), which also
#     allocates through Rust's allocator and has no seekable support.
#
# usage: ./backends.sh [REV]    (default: HEAD)
#
# This is how the report in the PR adding `rust-backend` was made: run it
# again to refresh it. It runs every group, with the record data (fetched if
# missing) and its posts as the `levels` corpus, through ab.sh, so AB_PREFIX
# and AB_WORK apply (AB_WORK defaults to target/backends here). It writes
# the comparison table to results.md and the chart to backends.svg, both in
# AB_WORK.
set -e
bench="$(cd "$(dirname "$0")" && pwd)"
rev=$(git -C "$bench" rev-parse --short "${1:-HEAD}")
data="$bench/data/stackexchange/coffee"
[ -d "$data" ] || "$bench/data/fetch-stackexchange.sh" coffee
export AB_WORK="${AB_WORK:-$bench/target/backends}"
export ZSTD_BENCH_RECORDS="$data"
export ZSTD_BENCH_CORPUS="$data/coffee-posts.jsonl"

"$bench/ab.sh" "$rev:seekable" "$rev:seekable,zstd/with-rust-allocator" "$rev:zstd/rust-backend" >&2
# ab.sh labels each build after its revision and features.
c="${rev}_seekable"
c_alloc="${rev}_seekable_zstd_with-rust-allocator"
port="${rev}_zstd_rust-backend"
python3 "$bench/compare.py" "$c,$c_alloc,$port" "$(uname -n)=$AB_WORK/criterion" > "$AB_WORK/results.md"
python3 "$bench/plot.py" "$AB_WORK/criterion" "$AB_WORK/backends.svg" \
    "$c=C library" "$c_alloc=C library, Rust allocator" "$port=libzstd-rs-sys"
echo "wrote $AB_WORK/results.md and $AB_WORK/backends.svg" >&2
