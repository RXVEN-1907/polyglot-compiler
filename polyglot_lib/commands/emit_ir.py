"""The 'emit-ir' command: LLVM IR (clang) or GIMPLE dumps (gcc)."""

import argparse
import shutil
import tempfile
from pathlib import Path

from polyglot_lib.backends import (
    _absolutize_path_flags, common_compile_args, invoke_compiler,
)
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_USAGE, PolyglotError, log_running, run_cmd, yellow,
)

ORDER = 40


def _emit_ir_one(tc, src, args, out_path):
    """Emit IR for one TU.  clang -> LLVM IR (.ll); gcc -> GIMPLE dump."""
    compiler = tc.compiler_for([src])
    extra = common_compile_args(tc, args, [src], for_emit=True)
    if tc.kind == "clang":
        cmd = [compiler] + extra + ["-S", "-emit-llvm", str(src)]
        if out_path:
            cmd += ["-o", str(out_path)]
        invoke_compiler(cmd, args, what="IR emission")
        dest = out_path or Path.cwd() / (Path(src).stem + ".ll")
        print("Wrote LLVM IR %s" % dest)
        return
    # GCC: GIMPLE dumps are written to the current directory, so run inside
    # a scratch dir and fish the dump out afterwards.  The compiler's cwd
    # changes, so relative user paths must be absolutized first.
    extra = _absolutize_path_flags(extra, Path.cwd())
    src_abs = Path(src).resolve()
    with tempfile.TemporaryDirectory() as tmpdir:
        cmd = [compiler] + extra + ["-fdump-tree-gimple", "-S", str(src_abs),
                                    "-o", str(Path(tmpdir) / "out")]
        log_running(cmd, args)
        proc = run_cmd(cmd, cwd=tmpdir)
        if proc.returncode != 0:
            raise PolyglotError("IR emission failed with exit code %d"
                                % proc.returncode, proc.returncode or EXIT_FAILURE)
        dumps = sorted(Path(tmpdir).glob("*.gimple"))
        if not dumps:
            raise PolyglotError("GCC produced no GIMPLE dump for %s" % src)
        dest = out_path or Path.cwd() / (Path(src).stem + ".gimple")
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(dumps[-1], dest)
        print(yellow("note: GCC backend emits GIMPLE (not LLVM IR) -> %s" % dest))


@command("emit-ir")
def cmd_emit_ir(tc, args):
    for src in [Path(p) for p in args.paths]:
        if not src.exists():
            raise PolyglotError("Source file not found: %s" % src)
    out = Path(args.output) if args.output else None
    if out is not None and len(args.paths) > 1:
        raise PolyglotError("-o with multiple sources is ambiguous; emit one "
                            "file at a time", EXIT_USAGE)
    ext = ".ll" if tc.kind == "clang" else ".gimple"
    seen = {}
    for src in [Path(p) for p in args.paths]:
        out_path = out
        if out is None:
            # duplicate stems would overwrite each other's default dump
            # (cwd/<stem><ext>); disambiguate with the parent directory
            stem = Path(src).stem
            if stem in seen and seen[stem].resolve() != Path(src).resolve():
                parts = [q for q in Path(src).parent.parts
                         if q not in ("/", ".")]
                prefix = "_".join(parts)
                name = ((prefix + "_") if prefix else "dup_") + stem + ext
                out_path = Path.cwd() / name
            seen[stem] = Path(src)
        _emit_ir_one(tc, src, args, out_path)


def register(subparsers, global_flags):
    p = subparsers.add_parser("emit-ir", help="Emit LLVM IR (clang) or GIMPLE (gcc)")
    p.add_argument("paths", nargs="+", help="source files")
    p.add_argument("-o", "--output", default=argparse.SUPPRESS, help="output file")
    global_flags.add_common_flags(p)
