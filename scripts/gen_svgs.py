#!/usr/bin/env python3
"""Generate docs/svg/*.svg - the non-animated diagrams, all vector, all
self-adapting dark/light through an internal media query, all single-file
(no dark/light twins needed).

  pipeline-metro.svg   the built-in compiler's stages as a metro line
  cache-chart.svg      the measured rebuild numbers as bar pairs
  architecture.svg     the crate map as a constellation

One accent per canvas, lavender everywhere else, faithful numbers.
Usage: python3 scripts/gen_svgs.py
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from theme import CANVAS_DARK, CANVAS_LIGHT, LAVENDER_DARK, LAVENDER_LIGHT, MODES

OUT = Path(__file__).resolve().parent.parent / "docs" / "svg"

ACC = MODES["transmute"]  # ochre - the pipeline's own accent


def css() -> str:
    return f''':root {{ --canvas: {CANVAS_DARK}; --text: #F5F6FA; --dim: #8A8FA3;
              --lav: {LAVENDER_DARK}; --acc: {ACC["dark"]}; --edge: #262933; --bit: #1C1F27; }}
    @media (prefers-color-scheme: light) {{
      :root {{ --canvas: {CANVAS_LIGHT}; --text: #1A1C23; --dim: #5C6070;
              --lav: {LAVENDER_LIGHT}; --acc: {ACC["light"]}; --edge: #DADBE1; --bit: #E4E5EB; }}
    }}'''


def metro_svg() -> str:
    stations = [
        ("source", "fib.c · 9 lines"),
        ("lex", "tokens"),
        ("parse", "AST · 7 nodes"),
        ("sema", "types · names"),
        ("ir", "12 instrs"),
        ("optimize", "const-fold → 9"),
        ("llvm", "fib.ll"),
        ("binary", "fib.o → ./fib"),
    ]
    n = len(stations)
    x0, x1, y = 90, 1130, 120
    step = (x1 - x0) / (n - 1)
    dots, labels = [], []
    for i, (name, note) in enumerate(stations):
        x = x0 + i * step
        accented = name == "optimize"
        ring = f'stroke="var(--acc)" stroke-width="4"' if accented else 'stroke="var(--lav)" stroke-width="3"'
        fill = 'var(--acc)' if accented else 'var(--canvas)'
        dots.append(f'<circle cx="{x:.0f}" cy="{y}" r="9" fill="{fill}" {ring}/>')
        above = i % 2 == 0
        ty = y - 44 if above else y + 52
        ny = ty + 24
        color = "var(--acc)" if accented else "var(--text)"
        labels.append(
            f'<text x="{x:.0f}" y="{ty}" text-anchor="middle" font-family="ui-monospace,monospace" '
            f'font-size="21" font-weight="bold" fill="{color}">{name}</text>'
            f'<text x="{x:.0f}" y="{ny}" text-anchor="middle" font-family="ui-monospace,monospace" '
            f'font-size="15" fill="var(--dim)">{note}</text>')
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1220 220" width="100%">
  <style>{css()}</style>
  <line x1="{x0}" y1="{y}" x2="{x1}" y2="{y}" stroke="var(--lav)" stroke-width="3"/>
  <line x1="{x0 + 4 * step:.0f}" y1="{y}" x2="{x0 + 5 * step:.0f}" y2="{y}" stroke="var(--acc)" stroke-width="5"/>
  {''.join(dots)}
  {''.join(labels)}
  <text x="{x0 - 34}" y="{y - 6}" text-anchor="end" font-family="ui-monospace,monospace" font-size="15" fill="var(--dim)">in:</text>
  <text x="{x1 + 14}" y="{y - 6}" font-family="ui-monospace,monospace" font-size="15" fill="var(--dim)">out</text>
</svg>
'''


def chart_svg() -> str:
    bars = [
        ("--no-cache (full recompile)", 2278, "var(--bit)", "var(--dim)"),
        ("warm .polyglot-cache", 873, "var(--lav)", "var(--text)"),
    ]
    max_ms = 2278
    rows = []
    y = 150
    for label, ms, fill, tcol in bars:
        w = 560 * ms / max_ms
        rows.append(
            f'<text x="60" y="{y - 12}" font-family="ui-monospace,monospace" font-size="18" fill="var(--dim)">{label}</text>'
            f'<rect x="60" y="{y}" width="{w:.0f}" height="40" rx="9" fill="{fill}"/>'
            f'<text x="{60 + w + 20:.0f}" y="{y + 27}" font-family="ui-monospace,monospace" font-size="24" '
            f'font-weight="bold" fill="{tcol}">{ms} ms</text>')
        y += 96
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 900 380" width="100%">
  <style>{css()}</style>
  <text x="60" y="58" font-family="ui-monospace,monospace" font-size="27" font-weight="bold" fill="var(--text)">incremental object cache</text>
  <text x="62" y="90" font-family="ui-monospace,monospace" font-size="17" fill="var(--lav)">same 61-TU rebuild, measured</text>
  {''.join(rows)}
  <text x="60" y="352" font-family="ui-monospace,monospace" font-size="22" font-weight="bold" fill="var(--acc)">2.8x faster</text>
  <text x="240" y="352" font-family="ui-monospace,monospace" font-size="17" fill="var(--dim)">unchanged TUs never recompile</text>
</svg>
'''


def constellation_svg() -> str:
    # (name, x, y, r, hub?)
    nodes = [
        ("polyglot-driver", 610, 175, 30, True),
        ("polyglot-syntax", 330, 90, 16, False),
        ("polyglot-sema", 330, 260, 16, False),
        ("polyglot-ir", 880, 90, 18, False),
        ("polyglot-codegen", 880, 260, 20, False),
        ("polyglot-protocol", 610, 330, 14, False),
        ("polyglot-core", 160, 175, 18, False),
        ("polyglot-clang", 1075, 175, 14, False),
    ]
    edges = [
        (0, 1), (0, 2), (0, 3), (0, 4), (0, 5),
        (1, 6), (2, 6), (3, 6), (4, 6),
        (7, 5),
    ]
    el = []
    for a, b in edges:
        x1, y1 = nodes[a][1], nodes[a][2]
        x2, y2 = nodes[b][1], nodes[b][2]
        el.append(f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="var(--edge)" stroke-width="1.5"/>')
    star = ('<g fill="var(--bit)">' +
            ''.join(f'<circle cx="{(i * 137) % 1180 + 20}" cy="{(i * 251) % 360 + 12}" r="{1.2 + (i % 3) * 0.7}"/>'
                    for i in range(46)) +
            '</g>')
    body = []
    for name, x, y, r, hub in nodes:
        if hub:
            body.append(f'<circle cx="{x}" cy="{y}" r="{r + 10}" fill="none" stroke="var(--lav)" stroke-width="1.5" opacity="0.5"/>')
        body.append(f'<circle cx="{x}" cy="{y}" r="{r}" fill="var(--canvas)" stroke="var(--lav)" stroke-width="3"/>')
        anchor, dy = ("middle", r + 26)
        body.append(f'<text x="{x}" y="{y + dy}" text-anchor="{anchor}" font-family="ui-monospace,monospace" '
                    f'font-size="17" font-weight="{"bold" if hub else "normal"}" fill="{"var(--text)" if hub else "var(--dim)"}">{name}</text>')
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1220 400" width="100%">
  <style>{css()}</style>
  <rect x="0" y="0" width="1220" height="400" fill="var(--canvas)" rx="14"/>
  {star}
  {''.join(el)}
  {''.join(body)}
  <text x="610" y="42" text-anchor="middle" font-family="ui-monospace,monospace" font-size="16" fill="var(--dim)">two front doors, one repo - the Rust driver is a compiler; the Python driver is every toolchain you have</text>
</svg>
'''


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "pipeline-metro.svg").write_text(metro_svg())
    (OUT / "cache-chart.svg").write_text(chart_svg())
    (OUT / "architecture.svg").write_text(constellation_svg())
    for p in sorted(OUT.glob("*.svg")):
        print(f"wrote {p.relative_to(OUT.parent)} ({p.stat().st_size} B)")


if __name__ == "__main__":
    main()
