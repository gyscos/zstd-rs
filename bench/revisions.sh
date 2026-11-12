#!/bin/sh
# Compares git revisions on this machine, over every group, and draws the
# result.
#
# usage: ./revisions.sh BASE[:FEATURES] REV[:FEATURES]...
#
# BASE is the reference. Each revision is resolved to its commit first, so
# the report names exactly what ran, and so running this again with the
# same commits redoes the same comparison. Like backends.sh, it runs every
# group with the record data (fetched if missing) and its posts as the
# `levels` corpus, through ab.sh, so AB_PREFIX applies. AB_WORK defaults to
# target/revisions here. It writes the comparison table to results.md and the
# chart to revisions.svg, both in AB_WORK.
#
# A revision can carry features after a colon, as with ab.sh, and the same
# revision can be compared with itself to measure the noise. AB_MACHINE names
# the machine in the table (its CPU, say), as with ab.sh.
#
# This is how the performance reports of fixes are made, for instance:
#
#   AB_MACHINE="Xeon E3-1270 v6" ./revisions.sh main my-branch
set -e
# Revisions are passed on as words: no globbing.
set -f
[ $# -ge 2 ] || { echo "usage: $0 BASE[:FEATURES] REV[:FEATURES]..." >&2; exit 1; }
bench="$(cd "$(dirname "$0")" && pwd)"
data="$bench/data/stackexchange/coffee"
[ -d "$data" ] || "$bench/data/fetch-stackexchange.sh" coffee
export AB_WORK="${AB_WORK:-$bench/target/revisions}"
export ZSTD_BENCH_RECORDS="$data"
export ZSTD_BENCH_CORPUS="$data/coffee-posts.jsonl"

specs=
for arg in "$@"; do
    rev=${arg%%:*}
    commit=$(git -C "$bench" rev-parse --short "$rev^{commit}")
    specs="$specs ${commit}${arg#"$rev"}"
done

mkdir -p "$AB_WORK"
# shellcheck disable=SC2086 # one word per revision
"$bench/ab.sh" $specs > "$AB_WORK/results.md"
# ab.sh numbers repeated revisions: take its labels, in the same order.
labels=$(cat "$AB_WORK/labels")
series=
seen=
for arg in "$@"; do
    label=${labels%%,*}
    labels=${labels#"$label"}
    labels=${labels#,}
    # One word per revision in the chart's legend, named as given, or after
    # its label when given more than once.
    name=$(echo "$arg" | tr ' ' _)
    case " $seen " in *" $name "*) name=$label ;; esac
    seen="$seen $name"
    series="$series $label=$name"
done
# shellcheck disable=SC2086
python3 "$bench/plot.py" "$AB_WORK/criterion" "$AB_WORK/revisions.svg" $series
echo "wrote $AB_WORK/results.md and $AB_WORK/revisions.svg" >&2
