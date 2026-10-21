#!/usr/bin/env python3
"""Generate the polyglot logo as SVG: the P monogram.

The mark: a bold P - stem (the pipe), top bar, paren-arc bowl, flat
return - with ONE small chevron nested in its counter. Every language
folds into one shape; the chevron is the language you are speaking
right now, so mode variants recolor only that glyph. Transparent
background; colors adapt dark/light through a prefers-color-scheme
media query, so one file works everywhere.

Usage: python3 scripts/gen_logo.py   (writes docs/logo*.svg)
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from theme import LAVENDER_DARK, LAVENDER_LIGHT, MODES

OUT = Path(__file__).resolve().parent.parent / "docs"


def logo_svg(accent_dark: str, accent_light: str) -> str:
    """The mark, 256x256, transparent canvas."""
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
  <style>
    :root {{ --lav: {LAVENDER_DARK}; --acc: {accent_dark}; }}
    @media (prefers-color-scheme: light) {{
      :root {{ --lav: {LAVENDER_LIGHT}; --acc: {accent_light}; }}
    }}
  </style>
  <!-- the pipe: shell -->
  <path d="M 100 48 V 208" stroke="var(--lav)" stroke-width="20" stroke-linecap="round" fill="none"/>
  <!-- the top bar -->
  <path d="M 100 58 H 152" stroke="var(--lav)" stroke-width="20" stroke-linecap="round" fill="none"/>
  <!-- the paren-arc bowl -->
  <path d="M 152 58 A 35 35 0 0 1 152 128" stroke="var(--lav)" stroke-width="20" stroke-linecap="round" fill="none"/>
  <!-- the return -->
  <path d="M 152 128 H 100" stroke="var(--lav)" stroke-width="20" stroke-linecap="round" fill="none"/>
  <!-- the chevron in the counter: the language being spoken -->
  <path d="M 146 81 L 124 95 L 146 109" stroke="var(--acc)" stroke-width="11" stroke-linecap="round" stroke-linejoin="round" fill="none"/>
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
