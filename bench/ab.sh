#!/bin/sh
# Benchmarks several git revisions against each other on this machine.
#
# usage: ./ab.sh [-r ROUNDS] [-f FILTER] REV[:FEATURES] REV[:FEATURES]...
#
# Each revision is checked out in its own worktree, with this directory's
# copy of the suite (so every revision runs the same benchmarks), and built
# once. The builds then run in alternation, round after round, so that the
# machine drifting over time shows up as noise between rounds instead of as
# a difference between revisions. The first revision is the reference.
#
# A revision can be built with other features, given after a colon: they
# replace the suite's defaults, as with `--no-default-features --features`.
# The same revision can then be compared with itself, for example with
# another backend: `main main:seekable,zstd/with-rust-allocator`.
#
#   -r ROUNDS   rounds to run (default 2; compare.py reads the first two)
#   -f FILTER   only run the benchmarks matching this regex
#
# Environment:
#   AB_WORK     scratch directory (default: target/ab)
#   AB_PREFIX   command to run the benchmarks under, e.g. "taskset -c 4"
#   ZSTD_BENCH_RECORDS  passed on to the suite, see README.md
#
# The suite must compile against every revision given, so it can only use
# APIs they all have.
set -e
rounds=2
filter=
while getopts r:f: opt; do
    case $opt in
        r) rounds=$OPTARG ;;
        f) filter=$OPTARG ;;
        *) exit 1 ;;
    esac
done
shift $((OPTIND - 1))
[ $# -ge 2 ] || { echo "usage: $0 [-r ROUNDS] [-f FILTER] REV[:FEATURES] REV[:FEATURES]..." >&2; exit 1; }

bench="$(cd "$(dirname "$0")" && pwd)"
repo="$(git -C "$bench" rev-parse --show-toplevel)"
work="${AB_WORK:-$bench/target/ab}"
mkdir -p "$work"
work="$(cd "$work" && pwd)"

labels=
for spec in "$@"; do
    rev=${spec%%:*}
    # Even an empty list after the colon turns the defaults off.
    no_defaults=
    features=
    case $spec in *:*) no_defaults=--no-default-features features=${spec#*:} ;; esac
    label=$(echo "$spec" | tr -c 'A-Za-z0-9_.\n-' '_')
    # Different revisions can sanitize to the same label (feature/foo and
    # feature_foo), and comparing a revision with itself is a useful way to
    # measure noise: number the repeats so nothing gets overwritten.
    base=$label
    n=2
    while echo ",$labels," | grep -qF ",$label,"; do
        label="$base-$n"
        n=$((n + 1))
    done
    labels="$labels${labels:+,}$label"
    src="$work/src-$label"
    if [ ! -d "$src" ]; then
        git -C "$repo" worktree add --detach "$src" "$rev"
        git -C "$src" submodule update --init --recursive
    else
        git -C "$src" checkout --detach "$rev"
        git -C "$src" submodule update --init --recursive
    fi
    rm -rf "$src/bench"
    mkdir -p "$src/bench"
    (cd "$bench" && tar cf - --exclude ./target --exclude ./Cargo.lock --exclude ./data/stackexchange .) |
        (cd "$src/bench" && tar xf -)
    echo "building $spec" >&2
    exe=$(cd "$src/bench" &&
        CARGO_TARGET_DIR="$work/target-$label" cargo bench --no-run $no_defaults ${features:+--features "$features"} --message-format=json |
        python3 -c 'import json, sys
for line in sys.stdin:
    m = json.loads(line)
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == "zstd":
        print(m["executable"])')
    cp "$exe" "$work/bench-$label"
done

export CRITERION_HOME="$work/criterion"
rm -rf "$CRITERION_HOME"
round=1
while [ $round -le $rounds ]; do
    for label in $(echo "$labels" | tr , ' '); do
        echo "round $round: $label" >&2
        $AB_PREFIX "$work/bench-$label" --bench --noplot \
            --warm-up-time 1 --measurement-time 3 \
            --save-baseline "$label.r$round" ${filter:+"$filter"} > "$work/log-$label.r$round.txt" 2>&1
    done
    round=$((round + 1))
done

python3 "$bench/compare.py" "$labels" "$(uname -n)=$CRITERION_HOME"
