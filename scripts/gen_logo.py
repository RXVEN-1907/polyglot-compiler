#!/usr/bin/env python3
"""Generate the polyglot logo as SVG: many source forms, one machine.

The mark: a stream of source glyphs (braces, parens, angle brackets -
every language's punctuation) flows diagonally through an accent
transformation diamond and condenses into a tidy byte grid. One file
adapts dark/light by itself through a prefers-color-scheme media query.
Mode variants swap only the accent diamond and its ring - lavender stays
put, and no two mode accents ever share a canvas.

Usage: python3 scripts/gen_logo.py   (writes docs/logo*.svg)
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from theme import CANVAS_DARK, CANVAS_LIGHT, LAVENDER_DARK, LAVENDER_LIGHT, MODES

OUT = Path(__file__).resolve().parent.parent / "docs"


def logo_svg(accent_dark: str, accent_light: str) -> str:
    """The mark, 256x256. Colors resolve per color scheme; the accent pair
    is the only thing that changes between modes."""
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
  <style>
    :root {{ --canvas: {CANVAS_DARK}; --lav: {LAVENDER_DARK}; --acc: {accent_dark}; --edge: #2A2D37; --bit: #23262F; }}
    @media (prefers-color-scheme: light) {{
      :root {{ --canvas: {CANVAS_LIGHT}; --lav: {LAVENDER_LIGHT}; --acc: {accent_light}; --edge: #D8D8DE; --bit: #E4E4EA; }}
    }}
  </style>
  <defs>
    <clipPath id="badge"><rect x="8" y="8" width="240" height="240" rx="52"/></clipPath>
    <linearGradient id="flow" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="var(--lav)"/>
      <stop offset="1" stop-color="var(--bit)"/>
    </linearGradient>
  </defs>
  <g clip-path="url(#badge)">
    <rect x="8" y="8" width="240" height="240" fill="var(--canvas)"/>
    <!-- the source glyphs: every language's punctuation, at varied angles -->
    <g fill="none" stroke="var(--lav)" stroke-width="7" stroke-linecap="round" stroke-linejoin="round" opacity="0.9">
      <!-- braces -->
      <path d="M 52 34 q -14 0 -14 14 v 8 q 0 12 -10 14 q 10 2 10 14 v 8 q 0 14 14 14"/>
      <!-- angle brackets -->
      <path d="M 88 46 l -16 18 l 16 18"/>
      <path d="M 116 82 l 16 -18 l -16 -18"/>
      <!-- parens -->
      <path d="M 64 106 q -12 16 0 32"/>
      <!-- semicolon -->
      <circle cx="118" cy="30" r="4.5" fill="var(--lav)" stroke="none"/>
      <path d="M 118 40 v 12"/>
      <!-- square brackets -->
      <path d="M 40 58 v 22 h 12 M 40 80 v -22"/>
    </g>
    <!-- flow: glyphs converge into the transformation diamond -->
    <g fill="none" stroke="url(#flow)" stroke-width="6" stroke-linecap="round" opacity="0.6">
      <path d="M 96 84 q 18 14 30 26"/>
      <path d="M 74 128 q 22 6 44 -6"/>
      <path d="M 60 152 q 26 -4 50 -22"/>
    </g>
    <!-- the transformation diamond: where compilation happens -->
    <rect x="118" y="118" width="40" height="40" rx="6" transform="rotate(45 138 138)" fill="var(--acc)"/>
    <rect x="110" y="110" width="56" height="56" rx="10" transform="rotate(45 138 138)" fill="none" stroke="var(--acc)" stroke-width="3" opacity="0.4"/>
    <!-- the machine: a tidy byte grid condensing out of the diamond -->
    <g fill="var(--bit)">
      {''.join(f'<rect x="{128 + col * 24}" y="{150 + row * 24}" width="12" height="12" rx="2.5" opacity="{0.85 - 0.14 * row}"/>'
               for row in range(4) for col in range(4)
               if (row * 4 + col) % 5 != 0)}
    </g>
    <rect x="8" y="8" width="240" height="240" rx="52" fill="none" stroke="var(--edge)" stroke-width="2"/>
  </g>
</svg>
'''


def main() -> None:
    OUT.mkdir(exist_ok=True)
    (OUT / "logo.svg").write_text(logo_svg(LAVENDER_DARK, LAVENDER_LIGHT))
    for mode, m in MODES.items():
        (OUT / f"logo-{mode}.svg").write_text(logo_svg(m["dark"], m["light"]))
    for p in sorted(OUT.glob("logo*.svg")):
        print(f"wrote {p.relative_to(OUT.parent)} ({p.stat().st_size} B)")


if __name__ == "__main__":
    main()
