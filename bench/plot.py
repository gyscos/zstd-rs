"""Draws criterion runs saved by `ab.sh` as a bar chart, in SVG.

usage: python3 plot.py CRITERION_DIR OUTPUT.svg LABEL=NAME LABEL=NAME...

The first label is the reference. For each benchmark, every other label gets
a bar: how much longer than the reference it took, from the medians of both
rounds (as compare.py computes it). Its error bar spans the lowest and highest
bounds of the confidence intervals of its two rounds, so it covers both
criterion's uncertainty and the drift between rounds. The grey band around
zero is the same span for the reference itself.

Benchmarks missing for any label (such as the seekable ones, for a backend
without seekable) are left out. Only uses the standard library.
"""
import itertools
import json
import os
import sys
from html import escape

# A categorical palette checked for color-vision deficiencies, in order.
COLORS = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6"]
SURFACE, TEXT, MUTED, BAND, GRID = "#fcfcfb", "#0b0b0b", "#52514e", "#e4e3df", "#ebeae6"


def load(root, names):
    """Returns {bench: {baseline: (median, low, high)}}."""
    results = {}
    for dirpath, _, files in os.walk(root):
        baseline = os.path.basename(dirpath)
        if "estimates.json" in files and baseline in names:
            bench = os.path.relpath(os.path.dirname(dirpath), root)
            with open(os.path.join(dirpath, "estimates.json")) as f:
                median = json.load(f)["median"]
            ci = median["confidence_interval"]
            results.setdefault(bench, {})[baseline] = (
                median["point_estimate"],
                ci["lower_bound"],
                ci["upper_bound"],
            )
    return results


def format_ns(ns):
    for unit, scale in (("s", 1e9), ("ms", 1e6), ("µs", 1e3)):
        if ns >= scale:
            return f"{ns / scale:.3g} {unit}"
    return f"{ns:.3g} ns"


def main():
    if len(sys.argv) < 5:
        sys.exit(__doc__)
    root, output = sys.argv[1], sys.argv[2]
    series = [arg.split("=", 1) for arg in sys.argv[3:]]
    labels = [label for label, _ in series]
    rounds = (".r1", ".r2")
    results = load(root, {label + r for label in labels for r in rounds})

    rows = []  # (bench, reference time, reference span, [(change, low, high)])
    for bench in sorted(results):
        runs = results[bench]
        if any(label + r not in runs for label in labels for r in rounds):
            continue
        ref = sum(runs[labels[0] + r][0] for r in rounds) / 2

        def span(label):
            mid = sum(runs[label + r][0] for r in rounds) / 2 / ref - 1
            low = min(runs[label + r][1] for r in rounds) / ref - 1
            high = max(runs[label + r][2] for r in rounds) / ref - 1
            return mid, low, high

        rows.append((bench, ref, span(labels[0]), [span(l) for l in labels[1:]]))
    if not rows:
        sys.exit("no benchmark has results for every label")
    if len(labels) - 1 > len(COLORS):
        sys.exit(f"at most {len(COLORS) + 1} labels")

    # Layout, in pixels.
    name_w, time_w, plot_w, margin = 330, 70, 520, 12
    bar_h, row_gap, group_gap = 9, 6, 24
    legend_h, axis_h = 22 * (len(labels) + 1), 30
    others = len(labels) - 1
    row_h = bar_h * others + row_gap

    values = [v for _, _, ref_span, spans in rows for s in [ref_span] + spans for v in s]
    lo = min(-0.05, min(values))
    hi = max(0.05, max(values))
    # The smallest round step that keeps the axis to 12 ticks, however wide.
    step = next(
        m * 10.0**e for e in itertools.count(-2) for m in (1, 2, 2.5, 5) if (hi - lo) / (m * 10.0**e) <= 12
    )
    lo = step * int(lo / step - (lo < 0))
    hi = step * int(hi / step + (hi > 0))
    x0 = margin + name_w + time_w

    def x(v):
        return x0 + (v - lo) / (hi - lo) * plot_w

    groups = []
    for row in rows:
        group = row[0].split("/")[0]
        if not groups or groups[-1][0] != group:
            groups.append((group, []))
        groups[-1][1].append(row)
    height = margin + legend_h + axis_h + sum(group_gap + len(g) * row_h for _, g in groups) + axis_h

    out = []
    width = x0 + plot_w + margin + 20
    out.append(
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" font-family="sans-serif" font-size="11" fill="{TEXT}">'
    )
    out.append(f'<rect width="{width}" height="{height}" fill="{SURFACE}"/>')

    # Legend.
    y = margin
    out.append(
        f'<text x="{margin}" y="{y + 11}" font-weight="bold">Time compared to '
        f"{escape(series[0][1])} (lower is faster)</text>"
    )
    for i, (_, name) in enumerate(series[1:]):
        y += 22
        out.append(f'<rect x="{margin}" y="{y}" width="14" height="10" rx="2" fill="{COLORS[i]}"/>')
        out.append(f'<text x="{margin + 20}" y="{y + 9}">{escape(name)}</text>')
    y += 22
    out.append(f'<rect x="{margin}" y="{y}" width="14" height="10" fill="{BAND}"/>')
    out.append(
        f'<text x="{margin + 20}" y="{y + 9}">{escape(series[0][1])}\'s own spread; '
        f"error bars: lowest to highest bound of both rounds' 95% intervals</text>"
    )
    y = margin + legend_h

    def axis(y, top):
        ticks = []
        n = int(round((hi - lo) / step))
        for i in range(n + 1):
            v = lo + i * step
            tx = x(v)
            ticks.append(f'<text x="{tx:.1f}" y="{y + (16 if top else 12)}" text-anchor="middle">{v * 100:+.0f}%</text>')
        return ticks

    out += axis(y, True)
    y += axis_h
    plot_top = y
    out.append(f'<text x="{x0 - 6}" y="{y - 4}" text-anchor="end" fill="{MUTED}">{escape(series[0][1])}</text>')

    grid = []
    for group, group_rows in groups:
        y += group_gap
        out.append(f'<text x="{margin}" y="{y - 7}" font-weight="bold">{escape(group)}</text>')
        for bench, ref, ref_span, spans in group_rows:
            name = bench.split("/", 1)[1] if "/" in bench else bench
            mid_y = y + row_h / 2 - row_gap / 2
            out.append(f'<text x="{margin + 8}" y="{mid_y + 4:.1f}">{escape(name)}</text>')
            out.append(f'<text x="{x0 - 6}" y="{mid_y + 4:.1f}" text-anchor="end" fill="{MUTED}">{format_ns(ref)}</text>')
            _, rlo, rhi = ref_span
            out.append(
                f'<rect x="{x(rlo):.1f}" y="{y}" width="{max(x(rhi) - x(rlo), 1):.1f}" '
                f'height="{row_h - row_gap}" fill="{BAND}"/>'
            )
            for i, (mid, elo, ehi) in enumerate(spans):
                by = y + i * bar_h
                left, right = sorted((x(0), x(mid)))
                color = COLORS[i]
                out.append(
                    f'<rect x="{left:.1f}" y="{by + 1}" width="{max(right - left, 1):.1f}" '
                    f'height="{bar_h - 2}" rx="2" fill="{color}"><title>{escape(series[i + 1][1])}: '
                    f"{mid * 100:+.1f}% ({elo * 100:+.1f}% to {ehi * 100:+.1f}%)</title></rect>"
                )
                cy = by + bar_h / 2
                out.append(
                    f'<path d="M{x(elo):.1f} {cy}H{x(ehi):.1f}M{x(elo):.1f} {cy - 3}v6M{x(ehi):.1f} {cy - 3}v6" '
                    f'stroke="{TEXT}" stroke-width="1" fill="none"/>'
                )
            y += row_h
    plot_bottom = y

    for i in range(int(round((hi - lo) / step)) + 1):
        v = lo + i * step
        grid.append(
            f'<line x1="{x(v):.1f}" y1="{plot_top}" x2="{x(v):.1f}" y2="{plot_bottom}" '
            f'stroke="{MUTED if abs(v) < 1e-9 else GRID}" stroke-width="1"/>'
        )
    # Grid lines go under everything else drawn in the plot area.
    out[2:2] = grid
    out += axis(plot_bottom, False)
    out.append("</svg>")
    with open(output, "w") as f:
        f.write("\n".join(out) + "\n")


main()
