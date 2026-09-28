"""The 'mca' command: analyze machine code with llvm-mca."""

import argparse
import sys
import tempfile
from pathlib import Path

from polyglot_lib.backends import common_compile_args, invoke_compiler
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, PolyglotError, run_cmd,
)

ORDER = 140


@command("mca")
def cmd_mca(tc, args):
    mca = tc.tool_or_error("llvm-mca", "machine code analysis")
    src = Path(args.file)
    if not src.exists():
        raise PolyglotError("file not found: %s" % src)
    cmd = [str(mca), "--iterations=%d" % max(1, args.iterations)]
    if getattr(args, "mcpu", None):
        cmd.append("--mcpu=" + args.mcpu)
    if src.suffix == ".s":
        cmd.append(str(src))
        proc = run_cmd(cmd, capture_output=True, text=True)
    else:
        with tempfile.TemporaryDirectory() as tmpdir:
            asm = Path(tmpdir) / (src.stem + ".s")
            compiler = tc.compiler_for([src])
            extra = common_compile_args(tc, args, [src])
            ccmd = [compiler] + extra + ["-S", str(src), "-o", str(asm)]
            invoke_compiler(ccmd, args, what="assembly emission for mca")
            cmd.append(str(asm))
            proc = run_cmd(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr or "")
        raise PolyglotError("llvm-mca failed with exit code %d" % proc.returncode,
                            proc.returncode or EXIT_FAILURE)
    sys.stdout.write(proc.stdout or "")


def register(subparsers, global_flags):
    p = subparsers.add_parser("mca", help="Analyze machine code with llvm-mca")
    p.add_argument("file", help=".c source or .s assembly")
    p.add_argument("--iterations", type=int, default=3,
                   help="analysis iterations (default: 3)")
    p.add_argument("--mcpu", help="target CPU for the analysis")
    global_flags.add_common_flags(p)
