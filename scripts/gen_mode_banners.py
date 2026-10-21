#!/usr/bin/env python3
"""Generate the three mode banners (dark + light twins): one banner per
accent mode, each pairing lavender with exactly ONE color - FORGE amber,
INSPECT slate green, TRANSMUTE crimson. These are the README's theme
gallery; no banner ever shows two accents.

Deterministic. Usage: python3 scripts/gen_mode_banners.py
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, str(Path(__file__).parent))
from theme import MODES, MONO, MONO_B, SANS_B, palette, hxc


# each mode banner shows its workflow: (headline, tagline, terminal lines)
LINES = {
    "forge": ("build it. run it.", "caching, parallelism, sanitizers - around the compiler you already have",
              [("$ polyglot build ./your-project -j 8", "dim"),
               ("61 TUs  ·  2.4s cold  ·  873 ms cached", "text")]),
    "inspect": ("read any binary.", "symbols, imports, sections, strings - correlated views",
                [("$ polyglot inspect ./a.out", "dim"),
                 ("nm · objdump · readelf · strings, one pane", "text")]),
    "transmute": ("watch it compile.", "the built-in pipeline, narrated step by step",
                  [("$ polyglot-driver transmute-internal fib.c", "dim"),
                   ("source → parse → sema → IR → LLVM → binary", "text")]),
}


def render(mode: str, light: bool, out: Path) -> None:
    p = palette(mode, light=light)
    W, H = 1280, 640
    img = Image.new("RGB", (W, H), hxc(p["canvas"]))
    d = ImageDraw.Draw(img)

    mono_b = lambda s: ImageFont.truetype(MONO_B, s)
    sans_b = lambda s: ImageFont.truetype(SANS_B, s)
    mono = ImageFont.truetype(MONO, 30)

    # mode name as the header pill
    d.rounded_rectangle((84, 96, 336, 176), radius=40, fill=hxc(p["accent"]))
    name = MODES[mode]["name"]
    nw = d.textlength(name, font=mono_b(44))
    d.text((84 + (252 - nw) / 2, 112), name, font=mono_b(44), fill=hxc(p["canvas"]))

    # the workflow headline + tagline + terminal strip
    headline, tagline, terms = LINES[mode]
    d.text((84, 230), headline, font=sans_b(52), fill=hxc(p["text"]))
    d.text((86, 300), tagline, font=sans_b(34), fill=hxc(p["lavender"]))

    y = 430
    d.rounded_rectangle((84, y - 22, 1196, y + 122), radius=16, fill=hxc(p["panel"]),
                        outline=hxc(p["edge"]), width=2)
    yy = y
    for text, role in terms:
        color = {"dim": p["dim"], "text": p["text"], "accent": p["accent"]}[role]
        d.text((116, yy), text, font=mono, fill=hxc(color))
        yy += 50

    out.parent.mkdir(parents=True, exist_ok=True)
    img.save(out, optimize=True)
    print(f"wrote {out} ({out.stat().st_size // 1024} KiB)")


def main() -> None:
    root = Path(__file__).resolve().parent.parent
    for mode in MODES:
        render(mode, light=False, out=root / "docs" / f"mode-{mode}.png")
        render(mode, light=True, out=root / "docs" / f"mode-{mode}-light.png")


if __name__ == "__main__":
    main()
