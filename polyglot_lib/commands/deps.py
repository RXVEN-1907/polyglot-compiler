"""The 'deps' command: emit Make-style dependencies (-MM)."""

import argparse
import sys
from pathlib import Path

from polyglot_lib.backends import common_compile_args
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, PolyglotError, run_cmd,
)

ORDER = 70


@command("deps")
def cmd_deps(tc, args):
    src = Path(args.file)
    if not src.exists():
        raise PolyglotError("Source file not found: %s" % src)
    compiler = tc.compiler_for([src])
    cmd = [compiler] + common_compile_args(tc, args, [src]) + ["-MM", str(src)]
    if getattr(args, "target_name", None):
        cmd += ["-MT", args.target_name]
    if args.output:
        cmd += ["-MF", args.output]
    proc = run_cmd(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr or "")
        raise PolyglotError("dependency generation failed with exit code %d"
                            % proc.returncode, proc.returncode or EXIT_FAILURE)
    if args.output:
        print("Wrote dependency file %s" % args.output)
    else:
        sys.stdout.write(proc.stdout or "")


def register(subparsers, global_flags):
    p = subparsers.add_parser("deps", help="Emit Make-style dependencies (-MM)")
    p.add_argument("file", help="source file")
    p.add_argument("-o", "--output", default=argparse.SUPPRESS,
                   help="write depfile here instead of stdout")
    p.add_argument("--target-name", metavar="NAME", help="rename the target (-MT)")
    global_flags.add_common_flags(p)
