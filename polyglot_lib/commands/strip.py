"""The 'strip' command: strip symbols from object files."""

import argparse
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError

ORDER = 170


@command("strip", pass_through=True)
def cmd_strip(tc, args):
    """Strip symbols using llvm-strip."""
    strip_path = tc.tool_or_error("llvm-strip", "stripping symbols")
    # Forward all arguments to llvm-strip
    import subprocess
    import sys

    cmd = [str(strip_path)] + list(args.paths or [])
    try:
        result = subprocess.run(cmd, check=True)
        sys.exit(result.returncode)
    except subprocess.CalledProcessError as e:
        sys.exit(e.returncode)
    except OSError as e:
        raise PolyglotError("failed to execute llvm-strip: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("strip", help="Strip symbols from object files")
    p.add_argument("paths", nargs="+", help="object files or executables")
    # Common flags that llvm-strip accepts
    p.add_argument("-F", action="store_true",
                   help="display llvm-strip help")
    p.add_argument("-g", "--strip-debug", action="store_true",
                   help="strip debugger symbols only")
    p.add_argument("-K", "--keep-symbol", metavar="SYMBOLNAME",
                   help="keep only symbol SYMBOLNAME")
    p.add_argument("-N", "--strip-symbol", metavar="SYMBOLNAME",
                   help="remove symbol SYMBOLNAME")
    p.add_argument("-o", "--output", metavar="FILE",
                   help="write stripped output to FILE")
    p.add_argument("-p", "--preserve-dates", action="store_true",
                   help="preserve access and modification timestamps")
    p.add_argument("-R", "--remove-section", metavar="SECTIONNAME",
                   help="remove section SECTIONNAME")
    p.add_argument("-s", "--strip-all", action="store_true",
                   help="strip all symbols")
    p.add_argument("-S", "--strip-unneeded", action="store_true",
                   help="strip all symbols not needed by relocs")
    p.add_argument("-w", "--wildcard", action="store_true",
                   help="treat symbol names as wildcards")
    p.add_argument("-x", "--discard-all", action="store_true",
                   help="discard all symbols")
    p.add_argument("-X", "--discard-locals", action="store_true",
                   help="discard compiler-generated local symbols")