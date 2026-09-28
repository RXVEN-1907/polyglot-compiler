"""The 'pgo' command: profile-guided optimization via clang's
-fprofile-instr-generate workflow.

Builds an instrumented binary, runs it on the user's --args (LLVM_PROFILE_FILE
lands in a tempdir), merges the raw profile with llvm-profdata, rebuilds with
-fprofile-instr-use and reports size/time deltas against a plain build at the
same opt level.  clang-only; the GCC backend exits 3 with a hint.
"""

import argparse
import json
import os
import shlex
import subprocess
import sys
import tempfile
import time
from pathlib import Path

from polyglot_lib.backends import common_compile_args, invoke_compiler
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_MISSING_TOOL, EXIT_USAGE, PolyglotError, green,
    log_running, run_cmd, yellow,
)

ORDER = 168

#: how often each binary is timed for the report; the minimum is reported
#: (one noisy outlier cannot manufacture a fake speedup).
TIME_RUNS = 3
RUN_TIMEOUT = 60


def parse_program_args(raw):
    """--args is one shell-like string, tokenized with shlex (never a shell)."""
    text = (raw or "").strip()
    if not text:
        return []
    try:
        return shlex.split(text)
    except ValueError as exc:
        raise PolyglotError("bad --args value: %s" % exc, EXIT_USAGE)


def binary_size(tc, path):
    """Binary size in bytes: llvm-size -B 'dec' total when available, else
    the file size on disk."""
    sz = tc.find_tool("llvm-size")
    if sz:
        try:
            proc = run_cmd([str(sz), "-B", str(path)], capture_output=True,
                           text=True)
            if proc.returncode == 0:
                lines = [l for l in (proc.stdout or "").splitlines()
                         if l.strip()]
                if len(lines) >= 2:
                    cols = lines[-1].split()
                    if len(cols) >= 4 and cols[3].isdigit():
                        return int(cols[3])
        except (OSError, subprocess.SubprocessError):
            pass
    try:
        return os.path.getsize(path)
    except OSError:
        return None


def best_time_ms(exe, prog_args, runs, timeout):
    """Minimum wall time over `runs` executions; returns (min_ms, times_ms).

    stdin is /dev/null and output is captured; a hung run is a friendly
    error, never a traceback.
    """
    times = []
    for _ in range(runs):
        t0 = time.perf_counter()
        try:
            run_cmd([str(exe)] + list(prog_args), capture_output=True,
                    text=True, stdin=subprocess.DEVNULL, timeout=timeout)
        except subprocess.TimeoutExpired:
            raise PolyglotError("program timed out after %ds (give --args "
                                "that finish, or use bench instead)"
                                % timeout, EXIT_FAILURE)
        times.append((time.perf_counter() - t0) * 1000.0)
    return min(times), times


def delta_pct(new, old):
    """Percentage change new vs old; None when it cannot be computed."""
    if new is None or not old:
        return None
    return (new - old) / old * 100.0


def _pct(value):
    return "n/a" if value is None else "%+.1f%%" % value


@command("pgo")
def cmd_pgo(tc, args):
    src = Path(args.file)
    if not src.exists():
        raise PolyglotError("Source file not found: %s" % src)
    if tc.kind != "clang":
        raise PolyglotError(
            "pgo uses clang's -fprofile-instr-generate workflow; the active "
            "backend is %s.\nHint: run 'polyglot --compiler clang pgo ...' "
            "(llvm-profdata ships alongside clang)." % (tc.kind or "unknown"),
            EXIT_MISSING_TOOL)
    profdata_tool = tc.tool_or_error("llvm-profdata",
                                     "profile-guided optimization")

    prog_args = parse_program_args(getattr(args, "args", None))
    out = Path(args.out) if getattr(args, "out", None) else \
        Path(src.stem + ".pgo")
    # absolute form for exec/size: a bare "hot.pgo" would PATH-lookup, not
    # resolve against the cwd
    out_run = out.resolve()
    extra = common_compile_args(tc, args, [str(src)])
    compiler = tc.compiler_for([str(src)])

    with tempfile.TemporaryDirectory(prefix="polyglot_pgo_") as tmp:
        tmpdir = Path(tmp)
        baseline = tmpdir / "baseline"
        instrumented = tmpdir / "instrumented"

        invoke_compiler([compiler] + extra + [str(src), "-o", str(baseline)],
                        args, what="baseline build")
        invoke_compiler([compiler] + extra + ["-fprofile-instr-generate",
                                             str(src), "-o", str(instrumented)],
                        args, what="instrumented build")

        profraw = tmpdir / "pgo.profraw"
        env = dict(os.environ, LLVM_PROFILE_FILE=str(profraw))
        t0 = time.perf_counter()
        try:
            proc = run_cmd([str(instrumented)] + prog_args, env=env,
                           capture_output=True, text=True,
                           stdin=subprocess.DEVNULL, timeout=RUN_TIMEOUT)
        except subprocess.TimeoutExpired:
            raise PolyglotError("instrumented program timed out after %ds; "
                                "give --args that finish" % RUN_TIMEOUT,
                                EXIT_FAILURE)
        train_ms = (time.perf_counter() - t0) * 1000.0
        if proc.returncode != 0:
            print(yellow("warning: instrumented run exited with code %d; "
                         "profile data is still used if it was written"
                         % proc.returncode))
        if not profraw.exists():
            raise PolyglotError(
                "instrumented run wrote no profile data (%s missing); give "
                "--args that exercise the hot path" % profraw, EXIT_FAILURE)

        profdata = tmpdir / "pgo.profdata"
        merge_cmd = [str(profdata_tool), "merge",
                     "-output=" + str(profdata), str(profraw)]
        log_running(merge_cmd, args)
        merge = run_cmd(merge_cmd, capture_output=True, text=True)
        if merge.returncode != 0:
            sys.stderr.write(merge.stderr or "")
            raise PolyglotError("llvm-profdata merge failed with exit code %d"
                                % merge.returncode)

        invoke_compiler([compiler] + extra
                        + ["-fprofile-instr-use=" + str(profdata),
                           str(src), "-o", str(out_run)],
                        args, what="PGO build")

        base_size = binary_size(tc, baseline)
        pgo_size = binary_size(tc, out_run)
        base_ms, base_times = best_time_ms(baseline, prog_args, TIME_RUNS,
                                           RUN_TIMEOUT)
        pgo_ms, pgo_times = best_time_ms(out_run, prog_args, TIME_RUNS,
                                         RUN_TIMEOUT)

    size_delta = delta_pct(pgo_size, base_size)
    time_delta = delta_pct(pgo_ms, base_ms)
    payload = {
        "file": str(src),
        "backend": tc.kind,
        "compiler_version": tc.version,
        "program_args": prog_args,
        "profile_runs": 1,
        "timed_runs": TIME_RUNS,
        "baseline": {
            "size_bytes": base_size,
            "min_ms": round(base_ms, 3),
            "times_ms": [round(t, 3) for t in base_times],
        },
        "pgo": {
            "size_bytes": pgo_size,
            "min_ms": round(pgo_ms, 3),
            "times_ms": [round(t, 3) for t in pgo_times],
            "path": str(out),
        },
        "size_delta_pct": None if size_delta is None else round(size_delta, 2),
        "time_delta_pct": None if time_delta is None else round(time_delta, 2),
    }
    if getattr(args, "json", False):
        print(json.dumps(payload, indent=2))
        return

    print("=" * 62)
    print("PGO REPORT %s" % src)
    print("=" * 62)
    print("backend: %s (%s)" % (tc.kind, tc.version))
    print("profile args: %s   timed runs per binary: %d (min reported)"
          % (shlex.join(prog_args) if prog_args else "(none)", TIME_RUNS))
    print("training run: %.1f ms" % train_ms)
    print()
    print("  %-8s %12s %14s" % ("", "size (B)", "time (ms)"))
    print("  %-8s %12s %14s" % ("base", _size(base_size), "%.2f" % base_ms))
    print("  %-8s %12s %14s" % ("pgo", _size(pgo_size), "%.2f" % pgo_ms))
    print("  %-8s %12s %14s" % ("delta",
                                _pct(size_delta), _pct(time_delta)))
    print()
    print(green("Wrote PGO-optimized binary %s" % out))


def _size(value):
    return "n/a" if value is None else format(value, ",")


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "pgo",
        help="Profile-guided optimization (clang -fprofile-instr workflow)")
    p.add_argument("file", help="source file to optimize")
    p.add_argument("--args", default="", metavar='"ARG ..."',
                   help="quoted arguments for every run of the program, "
                        "e.g. --args \"1000000\" (training and timing)")
    p.add_argument("--out", metavar="PATH",
                   help="where to write the optimized binary "
                        "(default: <source stem>.pgo)")
    p.add_argument("--json", action="store_true",
                   help="machine-readable JSON output")
    p.add_argument("--opt-level", choices=list(global_flags.OPT_MAP),
                   default=argparse.SUPPRESS, help="optimization level")
    global_flags.add_common_flags(p, link=True)
