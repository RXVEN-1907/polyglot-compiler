"""polyglot design tokens - the single source every visual asset renders from.

The system: lavender is the soul and appears everywhere. Each MODE pairs it
with exactly one accent; the other accents never appear on the same canvas.

    dark   canvas #0B0C10  text #F5F6FA  lavender #C3B1E1
    light  canvas #FAFAFC  text #1A1C23  lavender #6C5CE7

    FORGE      copper (dark #E29A6C / light #A85F32)   build, run, cache
    INSPECT    teal   (dark #54C6B8 / light #17877B)   binaries, disasm
    TRANSMUTE  ochre  (dark #DCAF4F / light #9C7418)   IR, the pipeline

Usage: sys.path.insert(0, str(Path(__file__).parent)); from theme import *
"""

from __future__ import annotations

CANVAS_DARK = "#0B0C10"
CANVAS_LIGHT = "#FAFAFC"
PANEL_DARK = "#14161C"
PANEL_LIGHT = "#F0F1F5"
EDGE_DARK = "#262933"
EDGE_LIGHT = "#DADBE1"
TEXT_DARK = "#F5F6FA"
TEXT_LIGHT = "#1A1C23"
DIM_DARK = "#8A8FA3"
DIM_LIGHT = "#5C6070"
LAVENDER_DARK = "#C3B1E1"
LAVENDER_LIGHT = "#6C5CE7"
LAVENDER_DEEP = "#9C86D9"  # mid stop for gradients, mode-independent

MODES = {
    "forge": {"name": "FORGE", "accent": "copper", "dark": "#E29A6C", "light": "#A85F32"},
    "inspect": {"name": "INSPECT", "accent": "teal", "dark": "#54C6B8", "light": "#17877B"},
    "transmute": {"name": "TRANSMUTE", "accent": "ochre", "dark": "#DCAF4F", "light": "#9C7418"},
}

MONO = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"
MONO_B = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"
SANS = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
SANS_B = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"


def palette(mode: str = "forge", light: bool = False) -> dict:
    """Resolve the full palette for one mode on one canvas."""
    m = MODES[mode]
    return {
        "canvas": CANVAS_LIGHT if light else CANVAS_DARK,
        "panel": PANEL_LIGHT if light else PANEL_DARK,
        "edge": EDGE_LIGHT if light else EDGE_DARK,
        "text": TEXT_LIGHT if light else TEXT_DARK,
        "dim": DIM_LIGHT if light else DIM_DARK,
        "lavender": LAVENDER_LIGHT if light else LAVENDER_DARK,
        "accent": m["light"] if light else m["dark"],
        "mode": m["name"],
    }


def hxc(c: str) -> tuple:
    c = c.lstrip("#")
    return tuple(int(c[i:i + 2], 16) for i in (0, 2, 4))
