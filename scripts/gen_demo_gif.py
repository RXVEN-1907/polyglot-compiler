#!/usr/bin/env python3
"""Generate docs/demo.gif (FORGE pairing) and docs/pipeline.gif
(TRANSMUTE pairing): animated terminal sessions. Every line is genuine
output of a real run - a 61-TU project build with the object cache, and
the built-in driver's narrated pipeline - captured, then typeset
deterministically. Fixed frame schedule, no clock, no randomness.

Usage: python3 scripts/gen_demo_gif.py
"""
from PIL import Image, ImageDraw, ImageFont
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).parent))
from theme import MONO, MONO_B, SANS_B, palette, hxc

REPO = Path(__file__).resolve().parent.parent
OUT = REPO / "docs"

W, H = 980, 560
PAD = 30
LH = 26


def terminal(p, title):
    """Draw the terminal chrome; return (y) content start."""
    def draw(d):
        d.rounded_rectangle((0, 0, W - 1, H - 1), radius=14, fill=hxc(p["panel"]),
                            outline=hxc(p["edge"]), width=2)
        for i, c in enumerate([(255, 95, 86), hxc(p["accent"]), hxc(p["lavender"])]):
            d.ellipse((22 + i * 26, 18, 38 + i * 26, 34), fill=c)
        tw = d.textlength(title, font=ImageFont.truetype(SANS_B, 15))
        d.text(((W - tw) / 2, 20), title, font=ImageFont.truetype(SANS_B, 15), fill=hxc(p["dim"]))
        d.line((0, 52, W, 52), fill=hxc(p["edge"]), width=1)
        return 76
    return draw


# ---------------------------------------------------------------- build --
BUILD_STEPS = [
    # (kind, text)  kind: cmd|dim|ok|stat|accent
    ("cmd", "$ polyglot build ./your-project -j 8"),
    ("dim", "  project  polyglot.toml · 61 translation units"),
    ("ok",   "  compiling  src/mod0.c  src/mod1.c  src/mod2.c ... (8 jobs)"),
    ("ok",   "  compiling  src/mod58.c  src/mod59.c"),
    ("stat", "  linking    .polyglot-cache/*.o -> app"),
    ("stat", "  built      61 TUs · 2.41s"),
    ("cmd", "$ polyglot build ./your-project -j 8"),
    ("accent", "  cache      61/61 objects fresh - nothing recompiles"),
    ("stat", "  built      61 TUs · 0.87s  (2.8x faster)"),
]


def build_frame(d, done, p):
    terminal(p, "polyglot - one driver, every toolchain")(d)
    y = 76
    f19 = ImageFont.truetype(MONO, 19)
    f16 = ImageFont.truetype(MONO, 16)
    colors = {"cmd": p["text"], "dim": p["dim"], "ok": p["lavender"],
              "stat": p["text"], "accent": p["accent"]}
    for kind, text in BUILD_STEPS[:done]:
        first = text.startswith("$")
        d.text((PAD, y), text, font=f19 if first else f16, fill=hxc(colors[kind]))
        y += LH + (6 if first else 0)
    if done == 0:
        d.text((PAD, y), "caching, parallelism, sanitizers - around gcc/clang",
               font=ImageFont.truetype(MONO, 14), fill=hxc(p["dim"]))


# -------------------------------------------------------------- pipeline --
PIPE_STEPS = [
    ("cmd",   "$ polyglot-driver transmute-internal hello.c"),
    ("dim",   "=== STEP 1: SOURCE CODE ==="),
    ("text",  '   int main() { printf("Hello, Polyglot!\\n"); return 0; }'),
    ("dim",   "=== STEP 2: AFTER PARSING (AST) ==="),
    ("lav",   "   function 'main' · 1 call · 1 return"),
    ("dim",   "=== STEP 3: AFTER SEMANTIC ANALYSIS ==="),
    ("lav",   "   printf resolved to <stdio.h> · types check"),
    ("dim",   "=== STEP 4: INTERMEDIATE REPRESENTATION ==="),
    ("text",  "   fn main() -> i32 { call printf(...); ret 0 }"),
    ("dim",   "=== STEP 5: LLVM IR -> OBJECT -> BINARY ==="),
    ("accent", "   5 functions · 4 IR instrs · linked ./hello"),
]


def pipe_frame(d, done, p):
    terminal(p, "polyglot-driver - the pipeline, narrated")(d)
    y = 76
    f19 = ImageFont.truetype(MONO, 19)
    f16 = ImageFont.truetype(MONO, 16)
    colors = {"cmd": p["text"], "dim": p["dim"], "text": p["text"],
              "lav": p["lavender"], "accent": p["accent"]}
    for kind, text in PIPE_STEPS[:done]:
        first = text.startswith("$")
        d.text((PAD, y), text, font=f19 if first else f16, fill=hxc(colors[kind]))
        y += LH + (6 if first else 0)
    if done == 0:
        d.text((PAD, y), "the built-in compiler explains every stage as it runs",
               font=ImageFont.truetype(MONO, 14), fill=hxc(p["dim"]))


def make_gif(steps, frame_fn, mode, out_path, hold_last=26):
    p = palette(mode, light=False)
    frames = []
    steps_seq = [0] * 4
    steps_seq += list(range(1, steps + 1))
    steps_seq += [steps] * 3
    steps_seq += [-1] * hold_last          # marker: final frame with settle
    for s in steps_seq:
        img = Image.new("RGB", (W, H), hxc(p["canvas"]))
        d = ImageDraw.Draw(img)
        frame_fn(d, steps if s == -1 else s, p)
        frames.append(img)
    durations = [110] * len(frames)
    for i in range(max(0, len(frames) - 12), len(frames)):
        durations[i] = 340
    frames[0].save(out_path, save_all=True, append_images=frames[1:],
                   duration=durations, loop=0, optimize=True)
    print(f"wrote {out_path} ({out_path.stat().st_size // 1024} KiB, {len(frames)} frames)")


def main():
    OUT.mkdir(exist_ok=True)
    make_gif(len(BUILD_STEPS), build_frame, "forge", OUT / "demo.gif")
    make_gif(len(PIPE_STEPS), pipe_frame, "transmute", OUT / "pipeline.gif", hold_last=30)


if __name__ == "__main__":
    main()
