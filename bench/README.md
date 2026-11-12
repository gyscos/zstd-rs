# zstd-rs benchmarks

Criterion benchmarks for the code the bindings add around zstd: buffer
plumbing (`WriteBuf`, `InBuffer`/`OutBuffer`), the `Read`/`Write` wrappers,
context reuse with `with_context`, dictionaries, the seekable format and
zstdmt. zstd's own speed is not what this measures - zstd's `fullbench` and
`zstd -b` do that - except in the `levels` group, which is there to compare
backends. Most inputs are small, where the wrappers' per-call cost shows the
most.

This is a standalone crate, outside the published packages, so criterion never
becomes a dependency of `zstd` or `zstd-safe` and their MSRV is unaffected.

```sh
cd bench
cargo bench                        # everything
cargo bench -- '^bulk/'            # one group
```

## Groups

| Group | What it covers |
|---|---|
| `bulk` | One-shot compression and decompression, into a `Vec` and a `Cursor`, 64 B to 4 MiB |
| `stream` | `encode_all`/`decode_all`, and the `Read`/`Write` wrappers driven with small calls |
| `with_context` | Many small messages through one re-used context |
| `safe_stream` | zstd-safe streaming in 64-byte chunks: mostly the cost of the buffer wrappers |
| `seekable` | Short reads at random and sequential offsets of a seekable archive |
| `zstdmt` | Multithreaded compression, streaming and one-shot |
| `records` | Real database rows, one at a time, without a dictionary and with raw and prepared dictionaries (needs data, see below) |
| `levels` | One-shot compression and decompression of a whole file at levels -5, 1, 3, 9 and 19 (needs data, see below) |

The built-in data is synthetic but deterministic. The `records` group runs
on real rows, from the `*.jsonl` files (one record per line) in the directory
named by `ZSTD_BENCH_RECORDS`, and is skipped when that is unset. To get some:

```sh
data/fetch-stackexchange.sh          # coffee.stackexchange.com, a 5 MB download
ZSTD_BENCH_RECORDS=data/stackexchange/coffee cargo bench -- '^records/'
```

That converts four tables of a Stack Exchange data dump (CC BY-SA 4.0) to
JSON, one row per line: votes (about 85 B per row), comments (300 B), users
(400 B) and posts (1.2 KB). Any other directory of `.jsonl` files works too,
such as samples of your own messages. Do not commit the data here.

The `levels` group runs on the file named by `ZSTD_BENCH_CORPUS`, and is
skipped when that is unset. The posts of the same dump make a good one:

```sh
ZSTD_BENCH_CORPUS=data/stackexchange/coffee/coffee-posts.jsonl cargo bench -- '^levels/'
```

The `seekable` feature, on by default, can be turned off for a backend that
lacks the seekable format.

## Comparing revisions

`ab.sh` builds several git revisions with this copy of the suite, then runs
them in alternation and prints a comparison:

```sh
./ab.sh main my-branch                          # everything
./ab.sh -f '^(bulk|records)/' main my-branch    # some groups
AB_PREFIX="taskset -c 4" ./ab.sh main my-branch # pinned to one core
```

The table names the machine after `uname -n`; set `AB_MACHINE` to name it
otherwise, after its CPU for instance.

Features can be given after a colon, to build a revision with them instead
of the suite's defaults (as `--no-default-features --features` would). This
compares the same revision with different features:

```sh
./ab.sh main main:seekable,zstd/with-rust-allocator
```

The suite has to compile against every revision compared, so it should only
use APIs they all have.

`compare.py` can also merge the results of several machines (copy each
machine's `target/ab/criterion` over first):

```sh
python3 compare.py main,my-branch njord=results/njord shiny=results/shiny
```

`revisions.sh` runs every group that way, with the record data, then writes
the table and the chart to `target/revisions`. It is how the performance
reports of fixes are made, and refreshes them:

```sh
AB_MACHINE="Xeon E3-1270 v6" ./revisions.sh BASE REV
```

`plot.py` draws the results as a bar chart with error bars, in SVG:

```sh
python3 plot.py target/ab/criterion chart.svg main=main my-branch="My branch"
```

### Reading the results

Each revision runs twice, alternated (A, B, A, B), and the medians of both
rounds are compared. The difference between a revision's two rounds is the
noise: how much the machine itself drifted. A change is only flagged (`**`)
when it is above both 1% and twice that noise.

A few things to keep in mind:

- At a few tens of nanoseconds per call, code layout alone can move a result
  by several percent, in either direction, on one CPU and not another.
  Confirm small changes on at least two machines before trusting them.
- Benchmarks that allocate large buffers per iteration (`decode_all` at
  4 MiB, for instance) can change by tens of percent depending on how the
  allocator was left by the benchmarks before. If one moves a lot, run it on
  its own (`-f`) before looking for a cause in the code.
- To find where a per-call change comes from, count instructions rather than
  time: `perf stat -e instructions` over a loop of a few million calls is
  deterministic, and `valgrind --tool=callgrind` gives it per function.

### Machine setup

Laptop and desktop CPUs drift a lot by default. On one, round-to-round noise
went from ±4-13% to ±0.1-2% with these together:

- the `performance` CPU governor
  (`echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor`),
- turbo boost off (`echo 0 | sudo tee /sys/devices/system/cpu/cpufreq/boost`
  with `amd-pstate`, `echo 1 | sudo tee /sys/devices/system/cpu/intel_pstate/no_turbo`
  with Intel's driver),
- background services stopped, and nothing else running: a CI job, or a
  local service answering requests, shows up as noise right away.

Pinning the benchmark to some cores (`AB_PREFIX="taskset -c ..."`) is
optional, and was not measured on its own. If you do, leave out the cores
that take device interrupts (the busy columns of `/proc/interrupts`: network
cards and disks, not necessarily core 0), keep their SMT siblings idle, and
leave the `zstdmt` group 4 cores. The noise column of the results says
whether it helped.

Remember to restore the settings afterwards.

## Comparing backends

`backends.sh` runs every group against the C library, the C library with
Rust's allocator, and the libzstd-rs-sys Rust port (`rust-backend`), all at
one revision, then writes the table and the chart to `target/backends`. It
is how the report in the PR adding `rust-backend` was made, and refreshes it:

```sh
./backends.sh                                # at HEAD
AB_MACHINE="Xeon E3-1270 v6" ./backends.sh   # named after its CPU
```

The machine setup above applies too.
