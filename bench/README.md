# zstd-rs benchmarks

Criterion benchmarks for the code the bindings add around zstd: buffer
plumbing (`WriteBuf`, `InBuffer`/`OutBuffer`), the `Read`/`Write` wrappers,
context reuse with `with_context`, dictionaries, the seekable format and
zstdmt. zstd's own speed is not what this measures - zstd's `fullbench` and
`zstd -b` do that. Most inputs are small, where the wrappers' per-call cost
shows the most.

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

## Comparing revisions

`ab.sh` builds several git revisions with this copy of the suite, then runs
them in alternation and prints a comparison:

```sh
./ab.sh main my-branch                          # everything
./ab.sh -f '^(bulk|records)/' main my-branch    # some groups
AB_PREFIX="taskset -c 4" ./ab.sh main my-branch # pinned to one core
```

The suite has to compile against every revision compared, so it should only
use APIs they all have.

`compare.py` can also merge the results of several machines (copy each
machine's `target/ab/criterion` over first):

```sh
python3 compare.py main,my-branch njord=results/njord shiny=results/shiny
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
went from ±4-13% to ±0.1-2% with:

- the `performance` CPU governor
  (`echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor`),
- turbo boost off (`echo 0 | sudo tee /sys/devices/system/cpu/cpufreq/boost`
  with `amd-pstate`, `intel_pstate/no_turbo` with Intel's driver),
- background services stopped,
- the benchmark pinned to one core, away from core 0 and with its SMT sibling
  idle (`AB_PREFIX="taskset -c 4"`). The `zstdmt` group needs 4 cores.

Remember to restore the settings afterwards.
