"""The 'nm' command: list symbols in object files and executables."""

import argparse
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError

ORDER = 150


@command("nm", pass_through=True)
def cmd_nm(tc, args):
    """List symbols using llvm-nm."""
    nm_path = tc.tool_or_error("llvm-nm", "listing symbols")
    # Forward all arguments to llvm-nm
    import subprocess
    import sys

    cmd = [str(nm_path)] + list(args.paths or [])
    try:
        result = subprocess.run(cmd, check=True)
        sys.exit(result.returncode)
    except subprocess.CalledProcessError as e:
        sys.exit(e.returncode)
    except OSError as e:
        raise PolyglotError("failed to execute llvm-nm: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("nm", help="List symbols in object files")
    p.add_argument("paths", nargs="*", help="object files or executables")
    # Common flags that llvm-nm accepts
    p.add_argument("-a", "--debug-syms", action="store_true",
                   help="show debugger-only symbols")
    p.add_argument("-g", "--extern-only", action="store_true",
                   help="show only external symbols")
    p.add_argument("-u", "--undefined-only", action="store_true",
                   help="show only undefined symbols")
    p.add_argument("--defined-only", action="store_true",
                   help="show only defined symbols")
    p.add_argument("-P", "--posix", action="store_true",
                   help="use POSIX.2 output format")
    p.add_argument("-f", "--format", choices=["sysv", "bsd", "posix"],
                   help="output format")
    p.add_argument("-p", "--no-sort", action="store_true",
                   help="do not sort symbols")
    p.add_argument("--radix", choices=["d", "o", "x"],
                   help="radix for numeric values (d=decimal, o=octal, x=hex)")