"""The 'diff-asm' command: compile each source at several optimization
levels and show a per-function unified diff of the resulting assembly.

Levels accept the driver's opt names (none/less/default/... ) or plain
shorthand digits (0-3) plus s/z for -Os/-Oz; consecutive levels are diffed
pairwise.  Purely read-only: every artifact lives in a temporary directory.
"""

import argparse
import copy
import difflib
import hashlib
import json
import re
import tempfile
from pathlib import Path
from typing import Dict, List, Any, Optional

from polyglot_lib.backends import OPT_MAP, common_compile_args, invoke_compiler
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_USAGE, PolyglotError, green, red, yellow,
)
from polyglot_lib.inspection_cache import get_inspection_cache

ORDER = 55

#: compiler flag -> the OPT_MAP key that reproduces it (lets the shared
#: common_compile_args machinery build per-level flag lists unchanged).
FLAG_TO_KEY = {flag: key for key, flag in OPT_MAP.items()}

_LEVEL_TOKEN = re.compile(r"^(?:[Oo]?(\d)|[Oo]?([sz]))$")
_GLOBL_RE = re.compile(r"^\s*\.(?:globl|global|weak)\s+([^\s,]+)")
_FUNC_TYPE_RE = re.compile(r"^\s*\.type\s+([^,\s]+),\s*[@%]function")
_LABEL_RE = re.compile(r"^([A-Za-z_$.][\w$.]*):")


def level_flag(token):
    """Shorthand level token ('0', 'O2', 's', 'aggressive', ...) -> -O flag.

    Only levels the driver's OPT_MAP can reproduce are accepted, so every
    token maps onto a known opt key (no -O9 surprises downstream).
    """
    tok = str(token).strip()
    if tok in OPT_MAP:
        return OPT_MAP[tok]
    m = _LEVEL_TOKEN.match(tok)
    if m:
        flag = "-O" + (m.group(1) or m.group(2))
        if flag in FLAG_TO_KEY:
            return flag
    raise PolyglotError(
        "invalid optimization level '%s' (use 0-3, s, z or names: %s)"
        % (token, ", ".join(OPT_MAP)), EXIT_USAGE)


def parse_levels(spec):
    """'0,2,3' / 'none,aggressive' -> [(label, flag, opt_key)], deduped by
    flag in first-seen order.  At least two distinct levels are required."""
    levels = []
    seen = set()
    for token in (spec or "").split(","):
        token = token.strip()
        if not token:
            continue
        flag = level_flag(token)
        if flag in seen:
            continue
        seen.add(flag)
        levels.append((token, flag, FLAG_TO_KEY[flag]))
    if len(levels) < 2:
        raise PolyglotError(
            "diff-asm needs at least two distinct optimization levels "
            "(--levels 0,2,3)", EXIT_USAGE)
    return levels


def function_names(lines):
    """Symbols the backend declares as functions (.globl/.weak plus
    .type ...,function), so static functions are covered too."""
    names = set()
    for line in lines:
        for rx in (_GLOBL_RE, _FUNC_TYPE_RE):
            m = rx.match(line)
            if m:
                names.add(m.group(1))
    return names


def split_functions(text):
    """Assembly text -> {function name: [chunk lines]}.

    A chunk starts at a column-0 label line naming a known function and
    runs until the next such label; local labels (.L1:) and directives in
    between stay in the chunk.  Falls back to one '<file>' chunk when no
    function is recognized (unknown targets, -masm quirks, ...).
    """
    lines = text.splitlines()
    names = function_names(lines)
    chunks = {}
    current = None
    buf = []
    for line in lines:
        m = _LABEL_RE.match(line)
        if m and m.group(1) in names:
            if current is not None:
                chunks[current] = buf
            current = m.group(1)
            buf = [line]
        elif current is not None:
            buf.append(line)
    if current is not None:
        chunks[current] = buf
    if not chunks:
        chunks["<file>"] = lines
    return chunks


def diff_stats(diff_lines):
    added = sum(1 for l in diff_lines
                if l.startswith("+") and not l.startswith("+++"))
    removed = sum(1 for l in diff_lines
                  if l.startswith("-") and not l.startswith("---"))
    return added, removed


def diff_function(a, b, name, label_a, label_b):
    """Unified diff of one function between two levels; a missing side
    (function inlined away / newly emitted) counts as added or removed."""
    a = a or []
    b = b or []
    lines = list(difflib.unified_diff(
        a, b,
        fromfile="%s (%s)" % (name, label_a),
        tofile="%s (%s)" % (name, label_b), lineterm=""))
    added, removed = diff_stats(lines)
    if a is None:
        note = "new at %s" % label_b
    elif b is None:
        note = "gone at %s" % label_b
    else:
        note = None
    return {"name": name, "added": added, "removed": removed,
            "changed": bool(lines), "note": note, "diff": lines}


def pair_diffs(chunks_a, chunks_b, label_a, label_b):
    names = sorted(set(chunks_a) | set(chunks_b))
    return [diff_function(chunks_a.get(n), chunks_b.get(n), n, label_a,
                          label_b) for n in names]


def paint_diff_line(line):
    if line.startswith("+") and not line.startswith("+++"):
        return green(line)
    if line.startswith("-") and not line.startswith("---"):
        return red(line)
    if line.startswith("@@"):
        return yellow(line)
    return line


def print_report(file_reports, level_labels):
    for rep in file_reports:
        print("=" * 62)
        print("ASM DIFF %s" % rep["file"])
        print("=" * 62)
        for pair in rep["pairs"]:
            print("%s %s -> %s" % (yellow("diff"), pair["from"], pair["to"]))
            funcs = pair["functions"]
            changed = [f for f in funcs if f["changed"]]
            if not changed:
                print("  %s (%d function(s) identical)"
                      % (green("identical"), len(funcs)))
                continue
            for f in funcs:
                label = f["name"] + (" (%s)" % f["note"] if f["note"] else "")
                if not f["changed"]:
                    print("  %s  %s" % (green("same"), label))
            for f in changed:
                label = f["name"] + (" (%s)" % f["note"] if f["note"] else "")
                print("  %s  %s  %s %s"
                      % (yellow("diff"), label, green("+%d" % f["added"]),
                         red("-%d" % f["removed"])))
            for line in f["diff"]:
                print("    " + paint_diff_line(line))
            print()


def _get_cache_key(sources, levels, intel):
    """Generate a cache key for diff-asm results."""
    key_data = {
        "sources": sorted([str(s) for s in sources]),
        "levels": [(label, flag, key) for label, flag, key in levels],
        "intel": intel,
    }
    key_str = json.dumps(key_data, sort_keys=True)
    return hashlib.sha256(key_str.encode()).hexdigest()[:16]


@command("diff-asm")
def cmd_diff_asm(tc, args):
    sources = [Path(p) for p in args.paths]
    for f in sources:
        if not f.exists():
            raise PolyglotError("Source file not found: %s" % f)
    levels = parse_levels(getattr(args, "levels", None))
    intel = getattr(args, "intel", False)
    as_json = getattr(args, "json", False) \
        or getattr(args, "output", "text") == "json"
    no_cache = getattr(args, "no_cache", False)

    # Check for cached results
    if as_json and not no_cache:
        cache_key = _get_cache_key(sources, levels, intel)
        cached = get_inspection_cache().get(
            "diff-asm",
            sources=[str(s) for s in sources],
            levels=[(label, flag, key) for label, flag, key in levels],
            intel=intel,
        )
        if cached is not None:
            print(json.dumps(cached, indent=2))
            return

    file_reports = []
    for src in sources:
        chunks_by_label = {}
        with tempfile.TemporaryDirectory(prefix="polyglot_diff_asm_") as tmp:
            tmpdir = Path(tmp)
            for i, (label, _flag, key) in enumerate(levels):
                level_args = copy.copy(args)
                level_args.opt_level = key
                asm = tmpdir / ("%s.%d.s" % (src.stem, i))
                chunks_by_label[label] = split_functions(
                    compile_asm(tc, src, level_args, intel, asm))
            pairs = []
            for (la, _fa, _ka), (lb, _fb, _kb) in zip(levels, levels[1:]):
                pairs.append({"from": la, "to": lb,
                              "functions": pair_diffs(chunks_by_label[la],
                                                      chunks_by_label[lb],
                                                      la, lb)})
            file_reports.append({"file": str(src), "pairs": pairs})

    if as_json:
        # Prepare JSON output
        payload = {
            "backend": tc.kind,
            "compiler_version": tc.version,
            "levels": [{"label": label, "flag": flag}
                       for label, flag, _ in levels],
            "files": file_reports,
        }

        # Cache the result
        if not no_cache:
            cache_key = _get_cache_key(sources, levels, intel)
            get_inspection_cache().set(
                "diff-asm",
                payload,
                sources=[str(s) for s in sources],
                levels=[(label, flag, key) for label, flag, key in levels],
                intel=intel,
            )

        print(json.dumps(payload, indent=2))
        return
    print_report(file_reports, [label for label, _, _ in levels])


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "diff-asm",
        help="Diff assembly across optimization levels")
    p.add_argument("paths", nargs="+", help="source files")
    p.add_argument("--levels", default="0,2,3",
                   help="comma-separated optimization levels: 0-3, s, z or "
                        "names (%s); default: 0,2,3"
                        % "/".join(OPT_MAP))
    p.add_argument("--intel", action="store_true",
                   help="Intel syntax (-masm=intel)")
    p.add_argument("--output", choices=["text", "json"], default="text",
                   help="report format (default: text)")
    p.add_argument("--json", action="store_true",
                   help="machine-readable JSON output (same as "
                        "--output json)")
    p.add_argument("--no-cache", action="store_true",
                   help="disable caching of inspection results")
    global_flags.add_common_flags(p)


def compile_asm(tc, source_path: Path, args, intel: bool, asm_path: Path) -> str:
    """Compile source to assembly and return the content."""
    import subprocess
    import os

    # Get compiler flags for the given optimization level
    flags = common_compile_args(tc, args)

    # Add Intel syntax flag if requested
    if intel:
        flags.append("-masm=intel")

    # Add flags to output assembly instead of object file
    flags.extend(["-S", "-o", str(asm_path)])

    # Add source file
    flags.append(str(source_path))

    # Run the compiler
    try:
        result = subprocess.run(
            [str(tc.compiler_for([source_path]))] + flags,
            capture_output=True, text=True, check=True
        )
        return asm_path.read_text()
    except subprocess.CalledProcessError as e:
        raise PolyglotError(
            f"compiler failed with return code {e.returncode}: {e.stderr}"
        )
    except OSError as e:
        raise PolyglotError(f"failed to execute compiler: {e}")