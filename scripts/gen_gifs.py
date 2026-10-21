#!/usr/bin/env python3
"""Generate the three session GIFs (docs/gifs/*.gif).

Each session is a real-shaped terminal run - commands TYPE themselves
character by character with a blinking cursor, output lines land one by
one, and each loop ends on a long hold. Every line is faithful to real
command behavior and the measured numbers (61-TU project: 2.41s cold /
0.873s warm; transmute's six stages match the driver's actual output
shape). One accent per session: forge copper, inspect teal, transmute
ochre - lavender everywhere else.

Deterministic frame schedule. Usage: python3 scripts/gen_gifs.py
"""
from PIL import Image, ImageDraw, ImageFont
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).parent))
from theme import MONO, MONO_B, SANS_B, palette, hxc

REPO = Path(__file__).resolve().parent.parent
OUT = REPO / "docs" / "gifs"

W, H = 980, 620
PAD = 34
TOP = 78
LH = 30

# ops: ("type", segments) | ("out", segments) | ("pause", ms) | ("gap",)
# segments: (text, role) with role in text|dim|lav|acc|ok
SESSIONS = {
    "forge": {
        "title": "polyglot - one driver, every toolchain",
        "mode": "forge",
        "ops": [
            ("type", [("$ polyglot build ./engine -j 8", "text")]),
            ("pause", 400),
            ("out", [("  project   polyglot.toml · 61 translation units", "dim")]),
            ("out", [("  compile    src/*.c · 8 jobs", "lav")]),
            ("out", [("             [████████████████████░░░░░░] 43/61", "dim")]),
            ("out", [("             [██████████████████████████] 61/61", "lav")]),
            ("out", [("  link       .polyglot-cache/*.o → ./engine", "dim")]),
            ("out", [("  built      61 TUs · ", "text"), ("2.41s", "acc"), (" cold", "text")]),
            ("pause", 700),
            ("type", [("$ polyglot build ./engine -j 8", "text")]),
            ("pause", 350),
            ("out", [("  cache      ", "dim"), ("61/61 objects fresh - nothing recompiles", "acc")]),
            ("out", [("  built      61 TUs · ", "text"), ("0.873s", "acc"), ("  (2.8x faster)", "text")]),
            ("pause", 500),
            ("type", [("$ ./engine --bench", "text")]),
            ("out", [("  14.2M ops/s · warm from cache", "lav")]),
            ("pause", 900),
        ],
    },
    "inspect": {
        "title": "polyglot inspect - the binutils zoo, one vocabulary",
        "mode": "inspect",
        "ops": [
            ("type", [("$ polyglot inspect ./engine", "text")]),
            ("pause", 400),
            ("out", [("  symbols 214 · imports 17 · sections 28 · strings 96", "dim")]),
            ("out", [("  ── correlated view ──────────────────────────", "lav")]),
            ("out", [("  T  run_pipeline   0x1a40   4024 B   .text", "text")]),
            ("out", [("  T  decode_frame   0x2b80   1832 B   .text", "text")]),
            ("out", [("  T  entropy_scan   0x33c0    984 B   .text", "text")]),
            ("out", [("  W  printf_chk     → libc.so.6   (import)", "dim")]),
            ("pause", 700),
            ("type", [("$ polyglot query ./engine run_pipeline --json", "text")]),
            ("out", [('  {"symbol":"run_pipeline","size":4024,', "dim")]),
            ("out", [('   "section":".text","refs":["decode_frame"]}', "dim")]),
            ("pause", 900),
        ],
    },
    "transmute": {
        "title": "polyglot-driver transmute-internal - watch it compile",
        "mode": "transmute",
        "ops": [
            ("type", [("$ polyglot-driver transmute-internal fib.c", "text")]),
            ("pause", 400),
            ("out", [("  [1/6] source      9 lines", "text")]),
            ("out", [("  [2/6] parse       AST ok · 7 nodes", "text")]),
            ("out", [("  [3/6] sema        types check · names resolve", "text")]),
            ("out", [("  [4/6] ir          12 instrs · const-fold → 9", "acc")]),
            ("out", [("  [5/6] llvm        fib.ll emitted", "text")]),
            ("out", [("  [6/6] object      llc → fib.o · linked ./fib", "text")]),
            ("pause", 700),
            ("type", [("$ ./fib", "text")]),
            ("out", [("  55", "acc")]),
            ("pause", 1000),
        ],
    },
}


def seg_color(p, role):
    return {"text": p["text"], "dim": p["dim"], "lav": p["lavender"],
            "acc": p["accent"], "ok": p["lavender"]}[role]


def terminal_frame(p, title, shown, typing=None):
    """shown: list of segment-lists already landed. typing: (segments, nchars)."""
    img = Image.new("RGB", (W, H), hxc(p["canvas"]))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((0, 0, W - 1, H - 1), radius=14, fill=hxc(p["panel"]),
                        outline=hxc(p["edge"]), width=2)
    for i, c in enumerate([(255, 95, 86), hxc(p["accent"]), hxc(p["lavender"])]):
        d.ellipse((22 + i * 26, 18, 38 + i * 26, 34), fill=c)
    tw = d.textlength(title, font=ImageFont.truetype(SANS_B, 15))
    d.text(((W - tw) / 2, 20), title, font=ImageFont.truetype(SANS_B, 15), fill=hxc(p["dim"]))
    d.line((0, 52, W, 52), fill=hxc(p["edge"]), width=1)

    f19 = ImageFont.truetype(MONO_B, 19)
    f16 = ImageFont.truetype(MONO, 16)
    fbar = ImageFont.truetype(MONO, 15)
    y = TOP
    for segs in shown:
        first = segs and segs[0][0].startswith("$")
        x = PAD
        for text, role in segs:
            f = f19 if first else (fbar if "█" in text else f16)
            d.text((x, y), text, font=f, fill=hxc(seg_color(p, role)))
            x += d.textlength(text, font=f)
        y += LH + (8 if first else 0)
    if typing is not None:
        segs, n = typing
        x = PAD
        full = "".join(t for t, _ in segs)
        part = full[:n]
        d.text((x, y), part, font=f19, fill=hxc(p["text"]))
        x += d.textlength(part, font=f19)
        # cursor
        d.rectangle((x + 2, y + 2, x + 14, y + 20), fill=hxc(p["accent"]))
    return img


def build_frames(ops, p):
    frames = []  # (PIL.Image, duration_ms)
    shown = []
    for op in ops:
        if op[0] == "type":
            segs = op[1]
            full_len = len("".join(t for t, _ in segs))
            for n in range(1, full_len + 1, 2):
                frames.append((terminal_frame(p, "", shown, (segs, n)), 26))
            frames.append((terminal_frame(p, "", shown, (segs, full_len)), 90))
            shown.append(segs)
        elif op[0] == "out":
            shown.append(op[1])
            frames.append((terminal_frame(p, "", shown), 240))
        elif op[0] == "pause":
            ms = op[1]
            # two cursor-blink frames at rest
            frames.append((terminal_frame(p, "", shown), min(ms, 500)))
            if ms > 500:
                frames.append((terminal_frame(p, "", shown, ([("", "dim")], 0)), 130))
                frames.append((terminal_frame(p, "", shown), ms - 630))
    return frames


def make_gif(name):
    s = SESSIONS[name]
    p = palette(s["mode"], light=False)
    frames = build_frames(s["ops"], p)
    # final long hold with two blinks
    frames.append((terminal_frame(p, "", [op[1] for op in s["ops"] if op[0] in ("type", "out")]), 1600))
    imgs, durs = zip(*frames)
    imgs[0].save(OUT / f"{name}.gif", save_all=True, append_images=imgs[1:],
                 duration=durs, loop=0, optimize=True)
    total = sum(durs) / 1000
    print(f"wrote {OUT / (name + '.gif')} ({(OUT / (name + '.gif')).stat().st_size // 1024} KiB, "
          f"{len(frames)} frames, {total:.1f}s loop)")


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for name in SESSIONS:
        make_gif(name)


if __name__ == "__main__":
    main()
