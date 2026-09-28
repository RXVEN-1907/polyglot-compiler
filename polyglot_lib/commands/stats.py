"""The 'stats' command: compilation statistics and time reports."""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from polyglot_lib.backends import common_compile_args
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, PolyglotError, log_running, run_cmd,
)

ORDER = 170

CLANG_TIME_ROW = re.compile(
    r"^\s+(?P<cols>(?:[\d.]+ \(\s*[\d.]+%\)\s*){2,4})(?P<phase>\S.*?)\s*$")
GCC_TIME_ROW = re.compile(r"^\s*(?P<phase>[A-Za-z][^:]*?)\s*:\s*"
                          r"(?P<secs>[\d.]+)\s*\(\s*(?P<pct>[\d.]+)%\)")
CLANG_TOTAL = re.compile(r"Total Execution Time:\s*([\d.]+)")


def parse_time_report(text, kind):
    """Best-effort parse of clang 'Pass execution timing report' or GCC
    'Time variable' tables into [{phase, seconds}]."""
    phases = []
    in_gcc_table = False
    for line in text.splitlines():
        if kind == "gcc":
            if "Time variable" in line:
                in_gcc_table = True
                continue
            if in_gcc_table:
                m = GCC_TIME_ROW.match(line)
                if m:
                    phases.append({"phase": m.group("phase").strip(),
                                   "seconds": float(m.group("secs"))})
                continue
        else:
            m = CLANG_TIME_ROW.match(line)
            if m:
                name = m.group("phase").strip()
                if name and not name.startswith("-"):
                    # columns: user, system, user+system, wall (each "s (pct)")
                    pairs = re.findall(r"([\d.]+) \(\s*[\d.]+%\)",
                                       m.group("cols"))
                    secs = float(pairs[2]) if len(pairs) > 2 else None
                    phases.append({"phase": name, "seconds": secs})
            mt = CLANG_TOTAL.search(line)
            if mt:
                phases.append({"phase": "TOTAL", "seconds": float(mt.group(1))})
    return phases


def binary_size(tc, path):
    """llvm-size -B total when available, else raw file size."""
    sz = tc.find_tool("llvm-size")
    if sz:
        try:
            proc = run_cmd([str(sz), "-B", str(path)], capture_output=True,
                           text=True)
            if proc.returncode == 0:
                lines = [l for l in proc.stdout.splitlines() if l.strip()]
                if len(lines) >= 2:
                    dec = lines[-1].split()
                    if len(dec) >= 4 and dec[3].isdigit():
                        return int(dec[3])
        except (OSError, subprocess.SubprocessError):
            pass
    try:
        return os.path.getsize(path)
    except OSError:
        return None


@command("stats")
def cmd_stats(tc, args):
    sources = [Path(p) for p in args.paths]
    for f in sources:
        if not f.exists():
            raise PolyglotError("Source file not found: %s" % f)

    has_json = getattr(args, "json", False)
    compiler = tc.compiler_for(sources)
    extra = common_compile_args(tc, args, sources, for_emit=True)
    extra.append("-ftime-report")
    if tc.kind == "gcc":
        extra = [f for f in extra if f != "-fno-discard-value-names"]

    t0 = time.time()
    cmd = [compiler] + extra + [str(f) for f in sources] + ["-o", "/dev/null"]
    if not has_json:           # keep stdout pure JSON in --json mode
        log_running(cmd, args)
    proc = run_cmd(cmd, capture_output=True, text=True)
    total_ms = int((time.time() - t0) * 1000)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr or "")
        raise PolyglotError("compilation failed with exit code %d"
                            % proc.returncode, proc.returncode or EXIT_FAILURE)

    timings = parse_time_report(proc.stderr or "", tc.kind)

    # Binary size at the requested optimization level.
    with tempfile.TemporaryDirectory() as tmpdir:
        tmp_exe = Path(tmpdir) / "pg_stats_out"
        bcmd = [compiler] + common_compile_args(tc, args, sources) \
            + [str(f) for f in sources] + ["-o", str(tmp_exe)]
        run_cmd(bcmd, capture_output=True)
        bsize = binary_size(tc, tmp_exe) if tmp_exe.exists() else None

    if has_json:
        payload = {
            "source": [str(f) for f in sources],
            "size_bytes": sum(f.stat().st_size for f in sources),
            "lines": sum(len(f.read_text(errors="replace").splitlines())
                         for f in sources),
            "target": getattr(args, "target", None) or "native",
            "opt_level": args.opt_level,
            "backend": tc.kind,
            "compiler_version": tc.version,
            "total_ms": total_ms,
            "binary_size_bytes": bsize,
            "timings": timings,
        }
        print(json.dumps(payload, indent=2))
        return

    print("=" * 60)
    print("COMPILATION STATISTICS")
    print("=" * 60)
    print("Source files: %d" % len(sources))
    for f in sources:
        size = f.stat().st_size
        lines = len(f.read_text(errors="replace").splitlines())
        print("  %s: %d bytes, %d lines" % (f, size, lines))

    print("\nTarget: %s" % (getattr(args, "target", None) or "native"))
    print("Optimization: %s" % args.opt_level)
    print("Backend: %s (%s)" % (tc.kind, tc.version))

    print("\n" + "-" * 60)
    print("COMPILATION TIME REPORT (%s format)" % tc.kind)
    print("-" * 60)
    for ph in timings:
        if ph.get("seconds") is not None:
            print("  %-40s %8.4f s" % (ph["phase"], ph["seconds"]))
        else:
            print("  %s" % ph["phase"])
    if not timings:
        print("  (no timing rows recognized - raw report on stderr above)")

    if bsize is not None:
        print("\nBinary size: %s bytes" % format(bsize, ","))
    print("Total wall time: %d ms" % total_ms)
    print("\n" + "=" * 60)


def register(subparsers, global_flags):
    p = subparsers.add_parser("stats", help="Compilation statistics")
    p.add_argument("paths", nargs="+", help="source files")
    p.add_argument("--target", default=argparse.SUPPRESS, help="target triple")
    p.add_argument("--opt-level", choices=list(global_flags.OPT_MAP),
                   default=argparse.SUPPRESS, help="optimization level")
    p.add_argument("--json", action="store_true",
                   help="machine-readable JSON output")
    global_flags.add_common_flags(p)
