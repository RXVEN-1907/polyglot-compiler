#!/usr/bin/env python3
"""Generate docs/cache-chart.png (+ light twin): what the incremental
object cache does to rebuild times, measured for real - 61-TU project,
`polyglot build` with a warm .polyglot-cache vs --no-cache. FORGE pairing.

Numbers measured on the dev box (gcc 13, python 3.12):
    cold (no cache)   2278 ms
    cached rebuild     873 ms
Rendered as a horizontal bar pair with the measured speedup.

Deterministic. Usage: python3 scripts/gen_charts.py
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).parent))
from theme import MONO, MONO_B, SANS_B, palette, hxc

BARS = [
    ("--no-cache (full recompile)", 2278, "dim"),
    ("warm .polyglot-cache", 873, "accent"),
]
SPEEDUP = "2.8x faster rebuild"


def render(p: dict, out: Path) -> None:
    W, H = 960, 420
    img = Image.new("RGB", (W, H), hxc(p["canvas"]))
    d = ImageDraw.Draw(img)

    sans_b = lambda s: ImageFont.truetype(SANS_B, s)
    mono = ImageFont.truetype(MONO, 24)
    mono_b = ImageFont.truetype(MONO_B, 30)

    d.text((60, 44), "incremental object cache", font=sans_b(36), fill=hxc(p["text"]))
    d.text((62, 92), "rebuild of the same 61-TU project, measured", font=sans_b(22), fill=hxc(p["lavender"]))

    max_ms = max(v for _, v, _ in BARS)
    bar_x, bar_w = 60, 620
    y = 170
    for label, ms, role in BARS:
        d.text((bar_x, y), label, font=mono, fill=hxc(p["dim"]))
        y += 40
        w = int(bar_w * ms / max_ms)
        color = p["accent"] if role == "accent" else p["edge"]
        d.rounded_rectangle((bar_x, y, bar_x + w, y + 44), radius=8, fill=hxc(color))
        d.text((bar_x + w + 24, y + 6), f"{ms} ms", font=mono_b, fill=hxc(p["text"]))
        y += 78

    # the measured takeaway
    d.rounded_rectangle((60, y - 6, 760, y + 58), radius=12, fill=hxc(p["panel"]),
                        outline=hxc(p["edge"]), width=2)
    d.text((88, y + 8), SPEEDUP, font=mono_b, fill=hxc(p["accent"]))
    d.text((88 + d.textlength(SPEEDUP, font=mono_b) + 28, y + 14),
           "unchanged TUs never recompile", font=mono, fill=hxc(p["dim"]))

    out.parent.mkdir(parents=True, exist_ok=True)
    img.save(out, optimize=True)
    print(f"wrote {out} ({out.stat().st_size // 1024} KiB)")


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    render(palette("forge", light=False), root / "docs" / "cache-chart.png")
    render(palette("forge", light=True), root / "docs" / "cache-chart-light.png")


if __name__ == "__main__":
    main()
