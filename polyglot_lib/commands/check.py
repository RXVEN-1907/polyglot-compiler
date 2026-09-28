"""The 'check' command: -fsyntax-only each source file."""

import argparse
from pathlib import Path

from polyglot_lib.backends import common_compile_args
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, PolyglotError, green, log_running, red, run_cmd,
)

ORDER = 30


@command("check")
def cmd_check(tc, args):
    failures = 0
    sources = [Path(p) for p in args.paths]
    for f in sources:
        if not f.exists():
            raise PolyglotError("Source file not found: %s" % f)
    for f in sources:
        print("Checking %s..." % f)
        compiler = tc.compiler_for([f])
        cmd = [compiler, "-fsyntax-only"] + common_compile_args(tc, args, [f])
        cmd += [str(f)]
        log_running(cmd, args)
        proc = run_cmd(cmd)
        if proc.returncode == 0:
            print(green("OK   %s" % f))
        else:
            print(red("FAIL %s" % f))
            failures += 1
    if failures:
        raise PolyglotError("syntax check failed for %d file(s)" % failures,
                            EXIT_FAILURE)
    print("All files passed syntax check")


def register(subparsers, global_flags):
    p = subparsers.add_parser("check", help="Check syntax without building")
    p.add_argument("paths", nargs="+", help="source files to check")
    global_flags.add_common_flags(p)
