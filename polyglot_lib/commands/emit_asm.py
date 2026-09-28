"""The 'emit-asm' command: emit assembly (-S), optionally Intel syntax."""

import argparse
from pathlib import Path

from polyglot_lib.backends import common_compile_args, invoke_compiler
from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError

ORDER = 50


@command("emit-asm")
def cmd_emit_asm(tc, args):
    for src in [Path(p) for p in args.paths]:
        if not src.exists():
            raise PolyglotError("Source file not found: %s" % src)
    compiler = tc.compiler_for(args.paths)
    extra = common_compile_args(tc, args, args.paths)
    if getattr(args, "intel", False):
        extra.append("-masm=intel")
    cmd = [compiler] + extra + [str(s) for s in args.paths] + ["-S"]
    if args.output:
        cmd += ["-o", args.output]
    invoke_compiler(cmd, args, what="assembly emission")
    if args.output:
        print("Wrote assembly %s" % args.output)
    else:
        for s in args.paths:
            print("Wrote assembly %s" % (Path.cwd() / (Path(s).stem + ".s")))


def register(subparsers, global_flags):
    p = subparsers.add_parser("emit-asm", help="Emit assembly")
    p.add_argument("paths", nargs="+", help="source files")
    p.add_argument("-o", "--output", default=argparse.SUPPRESS, help="output file")
    p.add_argument("--intel", action="store_true", help="Intel syntax (-masm=intel)")
    global_flags.add_common_flags(p)
