"""The 'preproc' command: run the preprocessor (-E)."""

import argparse
import sys
from pathlib import Path

from polyglot_lib.backends import common_compile_args, invoke_compiler
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, PolyglotError, run_cmd,
)

ORDER = 60


@command("preproc")
def cmd_preproc(tc, args):
    src = Path(args.file)
    if not src.exists():
        raise PolyglotError("Source file not found: %s" % src)
    compiler = tc.compiler_for([src])
    cmd = [compiler] + common_compile_args(tc, args, [src]) + ["-E", str(src)]
    if getattr(args, "no_lines", False):
        cmd.append("-P")
    if args.output:
        cmd += ["-o", args.output]
        invoke_compiler(cmd, args, what="preprocessing")
        print("Preprocessed %s -> %s" % (src, args.output))
        return
    proc = run_cmd(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr or "")
        raise PolyglotError("preprocessing failed with exit code %d"
                            % proc.returncode, proc.returncode or EXIT_FAILURE)
    sys.stdout.write(proc.stdout or "")


def register(subparsers, global_flags):
    p = subparsers.add_parser("preproc", help="Run the preprocessor (-E)")
    p.add_argument("file", help="source file")
    p.add_argument("-o", "--output", default=argparse.SUPPRESS,
                   help="write output to file instead of stdout")
    p.add_argument("--no-lines", action="store_true",
                   help="strip line markers (-P)")
    global_flags.add_common_flags(p)
