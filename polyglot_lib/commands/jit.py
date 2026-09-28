"""The 'jit' command: run a .ll/.bc file with lli (args after --)."""

import argparse
import sys
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_USAGE, PolyglotError, exit_status, run_cmd, split_program_args,
)

ORDER = 130


@command("jit", pass_through=True)
def cmd_jit(tc, args):
    lli = tc.tool_or_error("lli", "JIT execution of LLVM bitcode/IR")
    src = Path(args.file)
    if not src.exists():
        raise PolyglotError("file not found: %s" % src)
    if src.suffix not in (".ll", ".bc"):
        raise PolyglotError("jit expects a .ll or .bc file (use 'polyglot "
                            "emit-ir' first)", EXIT_USAGE)
    proc = run_cmd([str(lli), str(src)] + split_program_args(args.extra or []))
    sys.exit(exit_status(proc.returncode))


def register(subparsers, global_flags):
    p = subparsers.add_parser("jit", help="Run a .ll/.bc with lli (args after --)")
    p.add_argument("file", help="LLVM IR (.ll) or bitcode (.bc) file")
    p.set_defaults(extra=[])
