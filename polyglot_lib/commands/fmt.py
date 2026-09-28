"""The 'fmt' command: format code with clang-format."""

import argparse
import sys
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_USAGE, PolyglotError, green, red, run_cmd, exit_status,
)

ORDER = 100

FMT_SOURCE_EXTS = {".c", ".cc", ".cpp", ".h", ".hpp"}


@command("fmt")
def cmd_fmt(tc, args):
    cf = tc.tool_or_error("clang-format", "code formatting")
    if getattr(args, "dump_config", False):
        proc = run_cmd([str(cf), "-style=" + args.style, "-dump-config"],
                       capture_output=True, text=True)
        sys.stdout.write(proc.stdout)
        sys.exit(exit_status(proc.returncode))
    if not args.paths:
        raise PolyglotError("fmt: no files given", EXIT_USAGE)
    if args.check and args.in_place:
        raise PolyglotError("fmt: --check and -i cannot be combined",
                            EXIT_USAGE)
    # directories are expanded to their C/C++ sources so documented
    # invocations like `polyglot fmt --check src/` work
    files = []
    for f in args.paths:
        path = Path(f)
        if path.is_dir():
            matches = sorted(p for p in path.rglob("*")
                             if p.is_file() and p.suffix in FMT_SOURCE_EXTS)
            if not matches:
                raise PolyglotError(
                    "fmt: no C/C++ sources (*.c *.cc *.cpp *.h *.hpp) "
                    "found under %s" % path)
            files.extend(matches)
        else:
            if not path.exists():
                raise PolyglotError("file not found: %s" % path)
            files.append(path)
    changed = []
    for path in files:
        if args.in_place:
            proc = run_cmd([str(cf), "-style=" + args.style, "-i", str(path)])
            if proc.returncode != 0:
                raise PolyglotError("clang-format failed on %s" % path)
            print("formatted  %s" % path)
            continue
        proc = run_cmd([str(cf), "-style=" + args.style, str(path)],
                       capture_output=True, text=True)
        if proc.returncode != 0:
            sys.stderr.write(proc.stderr or "")
            raise PolyglotError("clang-format failed on %s" % path)
        if args.check:
            if proc.stdout != path.read_text():
                changed.append(str(path))
        else:
            sys.stdout.write(proc.stdout)
    if args.check:
        if changed:
            for f in changed:
                print(red("would reformat: %s" % f))
            raise PolyglotError("%d file(s) need formatting" % len(changed),
                                EXIT_FAILURE)
        print(green("all files already formatted"))


def register(subparsers, global_flags):
    p = subparsers.add_parser("fmt", help="Format code with clang-format")
    p.add_argument("paths", nargs="+", help="files to format")
    p.add_argument("--style", default="llvm",
                   choices=["llvm", "google", "chromium", "mozilla", "webkit",
                            "microsoft", "file"], help="clang-format style")
    p.add_argument("--check", action="store_true",
                   help="exit 1 if any file would change")
    p.add_argument("-i", dest="in_place", action="store_true",
                   help="format files in place")
    p.add_argument("--dump-config", action="store_true",
                   help="print the style config and exit")
