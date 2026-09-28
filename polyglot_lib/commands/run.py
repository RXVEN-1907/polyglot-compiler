"""The 'run' command: build a source file and execute it, passing any
tokens after '--' to the built program."""

import argparse
import sys
import tempfile
from pathlib import Path

from polyglot_lib.backends import common_compile_args, compile_to_exe
from polyglot_lib.commands import command
from polyglot_lib.commands.build import build_rust_project
from polyglot_lib.output import (
    PolyglotError, exit_status, run_cmd, split_program_args,
)

ORDER = 20


@command("run", pass_through=True)
def cmd_run(tc, args):
    sources = [Path(args.file)]
    if not sources[0].exists():
        raise PolyglotError("Source file not found: %s" % sources[0])
    args._project_dir = str(Path.cwd())
    with tempfile.TemporaryDirectory() as tmpdir:
        exe = Path(tmpdir) / "polyglot_run"
        if sources[0].suffix == ".rs":
            build_rust_project(tc, sources, exe, args)
        else:
            extra = common_compile_args(tc, args, sources)
            compile_to_exe(tc, sources, exe, args, extra)
        program = [str(exe)] + split_program_args(args.args or [])
        proc = run_cmd(program)
        sys.exit(exit_status(proc.returncode))


def register(subparsers, global_flags):
    p = subparsers.add_parser("run",
                              help="Build and run a source file (extra args after --)")
    p.add_argument("file", help="source file to run")
    global_flags.add_common_flags(p, link=True)
    p.add_argument("-j", "--jobs", type=int, default=1,
                   help="parallel per-TU compilation")
    p.add_argument("--no-cache", action="store_true",
                   help="bypass the incremental object cache")
    p.set_defaults(_pass_through=True)   # program args arrive via extras
