"""The 'tidy' command: lint with clang-tidy (extra args after --)."""

import argparse
import sys

from polyglot_lib.commands import command
from polyglot_lib.output import EXIT_FAILURE, EXIT_USAGE, PolyglotError, run_cmd

ORDER = 110


@command("tidy", pass_through=True)
def cmd_tidy(tc, args):
    tidy = tc.tool_or_error("clang-tidy", "static linting")
    if not args.paths:
        raise PolyglotError("tidy: no files given", EXIT_USAGE)
    rc = 0
    for f in args.paths:
        cmd = [str(tidy), str(f)]
        if getattr(args, "checks", None):
            cmd.append("--checks=" + args.checks)
        if getattr(args, "fix", False):
            cmd.append("--fix")
        cmd += list(args.extra or [])
        proc = run_cmd(cmd)
        rc = rc or proc.returncode
    sys.exit(rc if 0 <= rc < 256 else EXIT_FAILURE)


def register(subparsers, global_flags):
    p = subparsers.add_parser("tidy", help="Lint with clang-tidy (extra args after --)")
    p.add_argument("paths", nargs="*", help="files to lint")
    p.add_argument("--checks", help="checks string, e.g. '-*,modernize-*'")
    p.add_argument("--fix", action="store_true", help="apply suggested fixes")
    p.set_defaults(extra=[])
