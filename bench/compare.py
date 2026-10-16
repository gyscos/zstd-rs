"""Compares criterion runs saved as alternated baselines.

usage: python3 compare.py LABEL,LABEL[,...] [NAME=]CRITERION_DIR...

Each LABEL must have been saved as two baselines, LABEL.r1 and LABEL.r2
(two rounds, alternated with the other labels, as `ab.sh` does). The first label
is the reference. Several criterion directories (one per machine, say) can
be given, optionally named.

For each benchmark and label, the change is (r1 + r2) / (ref.r1 + ref.r2) - 1 on
the medians, and the noise is the largest round-to-round difference of any
label. A change is flagged with ** when it exceeds both 1% and twice the
noise: below that, it cannot be told apart from the machine's own drift.
"""
import json
import os
import sys


def load(root, names):
    results = {}
    for dirpath, _, files in os.walk(root):
        baseline = os.path.basename(dirpath)
        if "estimates.json" in files and baseline in names:
            bench = os.path.relpath(os.path.dirname(dirpath), root)
            with open(os.path.join(dirpath, "estimates.json")) as f:
                median = json.load(f)["median"]["point_estimate"]
            results.setdefault(bench, {})[baseline] = median
    return results


def format_ns(ns):
    for unit, scale in (("s", 1e9), ("ms", 1e6), ("µs", 1e3)):
        if ns >= scale:
            return f"{ns / scale:.3g} {unit}"
    return f"{ns:.3g} ns"


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    labels = sys.argv[1].split(",")
    rounds = (".r1", ".r2")
    names = {label + r for label in labels for r in rounds}
    machines = []
    for arg in sys.argv[2:]:
        name, _, path = arg.rpartition("=")
        machines.append((name or path, load(path, names)))

    reference = labels[0]
    print(
        "| benchmark | machine | "
        + reference
        + " | "
        + " | ".join(labels[1:])
        + " | noise |"
    )
    print("|---|---|---|" + "---|" * len(labels))
    benches = sorted(set().union(*[set(m) for _, m in machines]))
    for bench in benches:
        for name, results in machines:
            medians = results.get(bench, {})
            if len(medians) < len(names):
                continue
            ref = sum(medians[reference + r] for r in rounds)
            noise = max(
                abs(medians[label + ".r2"] / medians[label + ".r1"] - 1)
                for label in labels
            )
            cells = []
            for label in labels[1:]:
                change = sum(medians[label + r] for r in rounds) / ref - 1
                flag = " **" if abs(change) > max(0.01, 2 * noise) else ""
                cells.append(f"{change * 100:+.1f}%{flag}")
            print(
                f"| `{bench}` | {name} | {format_ns(ref / 2)} | "
                + " | ".join(cells)
                + f" | ±{noise * 100:.1f}% |"
            )


main()
