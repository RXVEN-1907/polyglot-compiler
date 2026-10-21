#!/usr/bin/env python3
"""Generate .github/social-preview.png (and its light twin): the repository
card - the P monogram, wordmark, tagline, and one live-looking build line.
Renders in the FORGE pairing (lavender + copper is the only accent on this
canvas). Deterministic.

Usage: python3 scripts/gen_social.py
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).parent))
from theme import MONO, MONO_B, SANS_B, palette, hxc


def mark(d: ImageDraw.ImageDraw, x: int, y: int, s: int, p: dict) -> None:
    """The P monogram, raster: pipe stem, top bar, paren bowl, accent chevron."""
    lw = max(10, s // 13)
    cx = x + s * 0.34          # stem x
    top = y + s * 0.06
    bot = y + s * 0.94
    bar_y = y + s * 0.10
    ret_y = y + s * 0.47
    bowl_r = s * 0.17
    # stem (pipe)
    d.line((cx, top, cx, bot), fill=hxc(p["lavender"]), width=lw)
    # top bar
    d.line((cx, bar_y, cx + s * 0.26, bar_y), fill=hxc(p["lavender"]), width=lw)
    # paren-arc bowl
    d.arc((cx + s * 0.26 - bowl_r, bar_y, cx + s * 0.26 + bowl_r, ret_y),
          start=-90, end=90, fill=hxc(p["lavender"]), width=lw)
    # return bar
    d.line((cx, ret_y, cx + s * 0.26, ret_y), fill=hxc(p["lavender"]), width=lw)
    # the chevron in the counter
    ch = s * 0.09
    ccx, ccy = cx + s * 0.22, (bar_y + ret_y) / 2
    d.line((ccx + ch * 0.7, ccy - ch, ccx - ch * 0.5, ccy), fill=hxc(p["accent"]), width=max(6, lw // 2))
    d.line((ccx - ch * 0.5, ccy, ccx + ch * 0.7, ccy + ch), fill=hxc(p["accent"]), width=max(6, lw // 2))


def render(p: dict, out: Path) -> None:
    W, H = 1280, 640
    img = Image.new("RGB", (W, H), hxc(p["canvas"]))
    d = ImageDraw.Draw(img)

    mark(d, 70, 150, 340, p)

    mono_b = lambda s: ImageFont.truetype(MONO_B, s)
    sans_b = lambda s: ImageFont.truetype(SANS_B, s)

    d.text((440, 168), "polyglot", font=mono_b(150), fill=hxc(p["text"]))
    d.text((446, 348), "many languages. one shape.", font=sans_b(40), fill=hxc(p["lavender"]))

    # a live-looking build line: lavender counts, one accent stat
    y = 452
    d.rounded_rectangle((440, y - 18, 1196, y + 96), radius=14, fill=hxc(p["panel"]), outline=hxc(p["edge"]), width=2)
    mono = ImageFont.truetype(MONO, 28)
    d.text((472, y), "$ polyglot build ./engine -j 8", font=mono, fill=hxc(p["dim"]))
    d.text((472, y + 44), "61 TUs parallel  ", font=mono, fill=hxc(p["text"]))
    tail = d.textlength("61 TUs parallel  ", font=mono)
    d.text((472 + tail, y + 44), "0.873s warm", font=mono, fill=hxc(p["accent"]))

    out.parent.mkdir(parents=True, exist_ok=True)
    img.save(out, optimize=True)
    print(f"wrote {out} ({out.stat().st_size // 1024} KiB)")


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    render(palette("forge", light=False), root / ".github" / "social-preview.png")
    render(palette("forge", light=True), root / ".github" / "social-preview-light.png")


if __name__ == "__main__":
    main()
