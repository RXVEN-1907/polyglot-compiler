"""The 'repl' command: interactive clang-repl (args after --)."""

import argparse
import os

from polyglot_lib.commands import command

ORDER = 120


@command("repl", pass_through=True)
def cmd_repl(tc, args):
    repl = tc.tool_or_error("clang-repl", "interactive REPL")
    os.execv(str(repl), [str(repl)] + list(args.extra or []))


def register(subparsers, global_flags):
    p = subparsers.add_parser("repl", help="Interactive clang-repl (args after --)")
    p.set_defaults(extra=[])
