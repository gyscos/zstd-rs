#!/bin/sh
# Counts the instructions the per-call workloads of instructions.rs execute,
# on several git revisions, with `perf stat -e instructions:u`.
#
# usage: ./instructions.sh [-r ROUNDS] REV[:FEATURES] REV[:FEATURES]...
#
# Revisions and features work as with ab.sh: each one is built from a
# worktree with this copy of the suite (the `zstd-safe-8` feature picks the
# names of revisions before zstd-safe 9). Unlike timings, instruction counts
# do not depend on code layout or on the machine's load, so a difference of
# a fraction of a percent is real. Worker threads are counted too.
#
#   ./instructions.sh main:seekable,zstd-safe-8 my-branch
#
# The work goes to AB_WORK (default target/instructions). Prints a markdown
# table: the first revision's counts, then each other one's difference.
set -e
set -f
rounds=300
while getopts r: opt; do
    case $opt in
        r) rounds=$OPTARG ;;
        *) exit 1 ;;
    esac
done
shift $((OPTIND - 1))
[ $# -ge 2 ] || { echo "usage: $0 [-r ROUNDS] REV[:FEATURES] REV[:FEATURES]..." >&2; exit 1; }
command -v perf >/dev/null || { echo "needs perf" >&2; exit 1; }

bench="$(cd "$(dirname "$0")" && pwd)"
repo="$(git -C "$bench" rev-parse --show-toplevel)"
work="${AB_WORK:-$bench/target/instructions}"
mkdir -p "$work"
work="$(cd "$work" && pwd)"

labels=
for spec in "$@"; do
    rev=${spec%%:*}
    no_defaults=
    features=
    case $spec in *:*) no_defaults=--no-default-features features=${spec#*:} ;; esac
    label=$(git -C "$repo" rev-parse --short "$rev^{commit}")$(echo "${spec#"$rev"}" | tr -c 'A-Za-z0-9_.\n-' '_')
    labels="$labels $label"
    src="$work/src-$label"
    if [ ! -d "$src" ]; then
        git -C "$repo" worktree add --detach "$src" "$rev" >&2
    else
        git -C "$src" checkout --detach "$rev" >&2
    fi
    git -C "$src" submodule update --init --recursive >&2
    rm -rf "$src/bench"
    mkdir -p "$src/bench"
    (cd "$bench" && tar cf - --exclude ./target --exclude ./Cargo.lock --exclude ./data/stackexchange .) |
        (cd "$src/bench" && tar xf -)
    echo "building $spec" >&2
    (cd "$src/bench" &&
        CARGO_TARGET_DIR="$work/target-$label" cargo build --release --bin instructions \
            $no_defaults ${features:+--features "$features"} >&2)
    cp "$work/target-$label/release/instructions" "$work/instructions-$label"
done

count() {
    perf stat -x, -e instructions:u "$1" "$2" "$rounds" 2>&1 >/dev/null |
        awk -F, '/instructions/ { print $1 }'
}

set -- $labels
first=$1
printf '| workload (%s rounds) |' "$rounds"
for label in "$@"; do printf ' %s |' "$label"; done
echo
printf '|---|'
for _ in "$@"; do printf -- '---|'; done
echo
for workload in $("$work/instructions-$first" list); do
    base=$(count "$work/instructions-$first" "$workload")
    printf '| `%s` | %s |' "$workload" "$base"
    for label in "$@"; do
        [ "$label" = "$first" ] && continue
        n=$(count "$work/instructions-$label" "$workload")
        printf ' %+.2f%% |' "$(echo "($n - $base) * 100 / $base" | bc -l)"
    done
    echo
done
