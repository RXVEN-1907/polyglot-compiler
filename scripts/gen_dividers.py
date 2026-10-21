#!/usr/bin/env python3
"""Generate docs/div-*.svg: slim section dividers.

A short accent bar at the left, a hairline that fades out to the right.
One accent per divider - forge copper, inspect teal, transmute ochre,
everything else lavender - and never two accents side by side in the
README's flow. Self-adapting dark/light, transparent background.

Usage: python3 scripts/gen_dividers.py
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from theme import LAVENDER_DARK, LAVENDER_LIGHT, MODES

OUT = Path(__file__).resolve().parent.parent / "docs" / "svg"


def divider_svg(accent_dark: str, accent_light: str) -> str:
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1200 40" width="100%" height="40">
  <style>
    :root {{ --acc: {accent_dark}; --hair: #262933; }}
    @media (prefers-color-scheme: light) {{
      :root {{ --acc: {accent_light}; --hair: #DADBE1; }}
    }}
  </style>
  <defs>
    <linearGradient id="fade" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="var(--hair)" stop-opacity="1"/>
      <stop offset="1" stop-color="var(--hair)" stop-opacity="0"/>
    </linearGradient>
  </defs>
  <rect x="0" y="16" width="64" height="7" rx="3.5" fill="var(--acc)"/>
  <rect x="80" y="18.5" width="1120" height="2" fill="url(#fade)"/>
</svg>
'''


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "div-lavender.svg").write_text(divider_svg(LAVENDER_DARK, LAVENDER_LIGHT))
    for mode, m in MODES.items():
        (OUT / f"div-{mode}.svg").write_text(divider_svg(m["dark"], m["light"]))
    for p in sorted(OUT.glob("div-*.svg")):
        print(f"wrote {p.relative_to(OUT.parent)} ({p.stat().st_size} B)")


if __name__ == "__main__":
    main()
