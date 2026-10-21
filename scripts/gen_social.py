#!/usr/bin/env python3
"""Generate .github/social-preview.png (and its light twin): the repository
card - polyglot badge, wordmark, tagline, and one live-looking build line.
Renders in the FORGE pairing (lavender + amber is the only accent on this
canvas); the mode banners carry the other pairings.

Deterministic: fixed layout, no clock, no randomness.

Usage: python3 scripts/gen_social.py
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).parent))
from theme import MONO, MONO_B, SANS_B, palette, hxc


def badge(d: ImageDraw.ImageDraw, x: int, y: int, size: int, p: dict) -> None:
    """The polyglot mark, raster form: glyphs -> diamond -> byte grid."""
    s = size
    d.rounded_rectangle((x, y, x + s, y + s), radius=s // 5, fill=hxc(p["panel"]),
                        outline=hxc(p["edge"]), width=3)
    # source glyphs: braces, angle brackets, parens - lavender strokes
    lw = max(4, s // 30)
    gx, gy = x + s * 0.16, y + s * 0.16
    u = s * 0.065
    d.arc((gx, gy, gx + 2.4 * u, gy + 2.4 * u), start=110, end=250, fill=hxc(p["lavender"]), width=lw)      # (
    d.arc((gx + 3.2 * u, gy, gx + 5.6 * u, gy + 2.4 * u), start=-70, end=70, fill=hxc(p["lavender"]), width=lw)  # )
    d.line((gx - 0.2 * u, gy + 3.4 * u, gx + 1.6 * u, gy + 4.6 * u), fill=hxc(p["lavender"]), width=lw)
    d.line((gx + 1.6 * u, gy + 4.6 * u, gx - 0.2 * u, gy + 5.8 * u), fill=hxc(p["lavender"]), width=lw)     # <
    # flow into the diamond
    fx, fy = x + s * 0.42, y + s * 0.52
    d.line((gx + 3.0 * u, gy + 3.0 * u, fx, fy), fill=hxc(p["edge"]), width=max(3, s // 40))
    d.line((gx - 0.4 * u, gy + 6.4 * u, fx - s * 0.05, fy + s * 0.05), fill=hxc(p["edge"]), width=max(3, s // 40))
    # the transformation diamond
    dm = s * 0.11
    d.polygon([(fx, fy - dm), (fx + dm, fy), (fx, fy + dm), (fx - dm, fy)], fill=hxc(p["accent"]))
    # the machine: byte grid condensing out of the diamond
    cell = s * 0.075
    bx0, by0 = fx + s * 0.055, fy - s * 0.045
    for row in range(4):
        for col in range(4):
            if (row * 4 + col) % 5 == 0:
                continue
            op = 0.9 - 0.18 * row
            c = tuple(int(v * op + hxc(p["edge"])[i] * (1 - op)) for i, v in enumerate(hxc(p["text"])))
            d.rounded_rectangle((bx0 + col * cell, by0 + row * cell,
                                 bx0 + col * cell + cell * 0.62, by0 + row * cell + cell * 0.62),
                                radius=2, fill=c)


def render(p: dict, out: Path) -> None:
    W, H = 1280, 640
    img = Image.new("RGB", (W, H), hxc(p["canvas"]))
    d = ImageDraw.Draw(img)

    badge(d, 84, 164, 312, p)

    mono_b = lambda s: ImageFont.truetype(MONO_B, s)
    sans_b = lambda s: ImageFont.truetype(SANS_B, s)

    d.text((452, 168), "polyglot", font=mono_b(150), fill=hxc(p["text"]))
    d.text((458, 348), "one driver. every toolchain.", font=sans_b(40), fill=hxc(p["lavender"]))

    # a live-looking build line: lavender counts, one accent stat
    y = 452
    d.rounded_rectangle((452, y - 18, 1196, y + 96), radius=14, fill=hxc(p["panel"]), outline=hxc(p["edge"]), width=2)
    mono = ImageFont.truetype(MONO, 28)
    d.text((484, y), "$ polyglot build ./your-project", font=mono, fill=hxc(p["dim"]))
    d.text((484, y + 44), "61 TUs parallel  ", font=mono, fill=hxc(p["text"]))
    tail = d.textlength("61 TUs parallel  ", font=mono)
    d.text((484 + tail, y + 44), "873 ms cached", font=mono, fill=hxc(p["accent"]))

    out.parent.mkdir(parents=True, exist_ok=True)
    img.save(out, optimize=True)
    print(f"wrote {out} ({out.stat().st_size // 1024} KiB)")


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    render(palette("forge", light=False), root / ".github" / "social-preview.png")
    render(palette("forge", light=True), root / ".github" / "social-preview-light.png")


if __name__ == "__main__":
    main()
