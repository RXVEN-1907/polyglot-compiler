"""The 'bench' command: build a source once (temp exe, default opt), then
time repeated runs of the binary with perf_counter and report
min/mean/stdev plus the build time.  Timings are wall-clock of the whole
process, so they include program startup.
"""

import argparse
import json
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

from polyglot_lib.backends import common_compile_args, compile_to_exe
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_USAGE, PolyglotError, quote, run_cmd,
    split_program_args, yellow,
)

ORDER = 165

#: --runs default and the "honest noise" threshold: when the sample stdev
#: exceeds this fraction of the mean, say so instead of pretending the
#: numbers are solid.
DEFAULT_RUNS = 5
NOISE_THRESHOLD = 0.20
RUN_TIMEOUT = 60


def time_runs(exe, prog_args, runs, timeout):
    """Run the binary `runs` times; returns (times_ms, exit_codes, timed_out).

    stdin is /dev/null and output is captured so repeated runs cannot spam
    or block the bench itself.
    """
    times = []
    codes = []
    for _ in range(runs):
        t0 = time.perf_counter()
        try:
            proc = run_cmd([str(exe)] + list(prog_args), capture_output=True,
                           text=True, stdin=subprocess.DEVNULL,
                           timeout=timeout)
        except subprocess.TimeoutExpired:
            return times, codes, True
        times.append((time.perf_counter() - t0) * 1000.0)
        codes.append(proc.returncode)
    return times, codes, False


def summarize(times):
    """min/mean/stdev in ms; stdev is the sample stdev (0.0 for one run)."""
    if not times:
        return {"min": None, "mean": None, "stdev": None}
    return {
        "min": min(times),
        "mean": statistics.fmean(times),
        "stdev": statistics.stdev(times) if len(times) > 1 else 0.0,
    }


def is_noisy(stats):
    return (stats["mean"] is not None and stats["mean"] > 0
            and stats["stdev"] is not None
            and stats["stdev"] > NOISE_THRESHOLD * stats["mean"])


def _ms(value):
    return "n/a" if value is None else "%.2f ms" % value


@command("bench", pass_through=True)
def cmd_bench(tc, args):
    src = Path(args.file)
    if not src.exists():
        raise PolyglotError("Source file not found: %s" % src)
    runs = args.runs
    if runs < 1:
        raise PolyglotError("--runs must be >= 1", EXIT_USAGE)

    # program arguments: repeatable --args plus any tokens after '--'
    prog_args = [str(a) for a in (getattr(args, "args", None) or [])] \
        + [str(a) for a in split_program_args(getattr(args, "extra", None)
                                              or [])]
    args._project_dir = str(Path.cwd())

    with tempfile.TemporaryDirectory(prefix="polyglot_bench_") as tmp:
        exe = Path(tmp) / "bench_out"
        extra = common_compile_args(tc, args, [str(src)])
        t0 = time.perf_counter()
        compile_to_exe(tc, [src], exe, args, extra)
        build_ms = (time.perf_counter() - t0) * 1000.0

        times, codes, timed_out = time_runs(exe, prog_args, runs,
                                            max(1, args.timeout))
        exe_str = str(exe)

    if timed_out:
        raise PolyglotError(
            "program timed out after %ds (use --timeout to raise it)"
            % max(1, args.timeout), EXIT_FAILURE)

    stats = summarize(times)
    failed = sorted({c for c in codes if c != 0})
    if failed and len(codes) == len([c for c in codes if c != 0]):
        raise PolyglotError(
            "program exited non-zero in all %d run(s) (codes: %s); fix it "
            "before benchmarking" % (len(codes), ", ".join(map(str, failed))),
            EXIT_FAILURE)

    noisy = is_noisy(stats)
    payload = {
        "file": str(src),
        "backend": tc.kind,
        "compiler_version": tc.version,
        "opt_level": getattr(args, "opt_level", "default"),
        "runs": len(times),
        "program_args": prog_args,
        "build_ms": round(build_ms, 1),
        "times_ms": [round(t, 3) for t in times],
        "exit_codes": codes,
        "min_ms": None if stats["min"] is None else round(stats["min"], 3),
        "mean_ms": None if stats["mean"] is None else round(stats["mean"], 3),
        "stdev_ms": None if stats["stdev"] is None else round(stats["stdev"], 3),
        "noisy": noisy,
    }
    if getattr(args, "json", False):
        print(json.dumps(payload, indent=2))
        return

    print("=" * 62)
    print("BENCH %s" % src)
    print("=" * 62)
    print("backend: %s (%s)   opt: %s" % (tc.kind, tc.version,
                                          payload["opt_level"]))
    print("build: %.1f ms" % build_ms)
    print("runs: %d%s" % (len(times),
                          "   args: %s" % quote(prog_args) if prog_args else ""))
    print()
    print("  min   %s" % _ms(stats["min"]))
    print("  mean  %s" % _ms(stats["mean"]))
    print("  stdev %s" % _ms(stats["stdev"]))
    for code in failed:
        print(yellow("warning: some runs exited with code %d; timings "
                     "include them" % code))
    if noisy:
        pct = 100.0 * stats["stdev"] / stats["mean"]
        print(yellow("warning: timings are noisy (stdev is %.0f%% of the "
                     "mean); try more --runs or a quieter machine" % pct))


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "bench",
        help="Build once, then time repeated program runs")
    p.add_argument("file", help="source file to build and time")
    p.add_argument("--runs", type=int, default=DEFAULT_RUNS,
                   help="number of timed runs (default: %d)" % DEFAULT_RUNS)
    p.add_argument("--args", action="append", default=[], metavar="ARG",
                   help="argument passed to the program (repeatable); "
                        "tokens after '--' also pass through")
    p.add_argument("--timeout", type=int, default=RUN_TIMEOUT,
                   help="per-run timeout in seconds (default: %d)"
                        % RUN_TIMEOUT)
    p.add_argument("--no-cache", action="store_true",
                   help="bypass the incremental object cache")
    p.add_argument("--json", action="store_true",
                   help="machine-readable JSON output")
    p.add_argument("--opt-level", choices=list(global_flags.OPT_MAP),
                   default=argparse.SUPPRESS, help="optimization level")
    global_flags.add_common_flags(p, link=True)
    p.set_defaults(_pass_through=True)   # program args arrive via extras
