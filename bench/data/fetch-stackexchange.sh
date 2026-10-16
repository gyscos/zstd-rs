#!/bin/sh
# Downloads a Stack Exchange data dump and converts some of its tables to
# JSON lines - one database row per line - for the `records` benchmarks.
#
# usage: data/fetch-stackexchange.sh [SITE]    (default: coffee)
#
# Then run the benchmarks with ZSTD_BENCH_RECORDS=data/stackexchange/SITE.
#
# The dumps are published under CC BY-SA 4.0 by Stack Exchange, Inc. They
# are downloaded for local use only and must not be committed here. Needs
# curl, 7z and python3.
set -e
site="${1:-coffee}"
dir="$(dirname "$0")/stackexchange"
archive="$dir/$site.stackexchange.com.7z"
mkdir -p "$dir/$site.xml" "$dir/$site"
if [ ! -f "$archive" ]; then
    curl -fL --progress-bar -o "$archive.part" \
        "https://archive.org/download/stackexchange/$site.stackexchange.com.7z"
    mv "$archive.part" "$archive"
fi
7z x -y -o"$dir/$site.xml" "$archive" Votes.xml Comments.xml Users.xml Posts.xml > /dev/null
for table in Votes Comments Users Posts; do
    lower=$(echo "$table" | tr '[:upper:]' '[:lower:]')
    python3 -I "$(dirname "$0")/se2jsonl.py" \
        "$dir/$site.xml/$table.xml" "$dir/$site/$site-$lower.jsonl"
done
