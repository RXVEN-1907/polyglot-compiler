"""The 'size' command: show section sizes of object files."""

import argparse
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError

ORDER = 155


@command("size", pass_through=True)
def cmd_size(tc, args):
    """Show section sizes using llvm-size."""
    size_path = tc.tool_or_error("llvm-size", "showing section sizes")
    # Forward all arguments to llvm-size
    import subprocess
    import sys

    cmd = [str(size_path)] + list(args.paths or [])
    try:
        result = subprocess.run(cmd, check=True)
        sys.exit(result.returncode)
    except subprocess.CalledProcessError as e:
        sys.exit(e.returncode)
    except OSError as e:
        raise PolyglotError("failed to execute llvm-size: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("size", help="Show section sizes of object files")
    p.add_argument("paths", nargs="*", help="object files or executables")
    # Common flags that llvm-size accepts
    p.add_argument("-A", "--radix", choices=["d", "o", "x"],
                   help="radix for numeric values (d=decimal, o=octal, x=hex)")
    p.add_argument("-B", "--berkeley", action="store_true",
                   help="use Berkeley format (default)")
    p.add_argument("-d", "--decimal", action="store_const", const="d",
                   dest="radix", help="decimal radix (same as -Ad)")
    p.add_argument("-x", "--hexadecimal", action="store_const", const="x",
                   dest="radix", help="hexadecimal radix (same as -Ax)")
    p.add_argument("-o", "--octal", action="store_const", const="o",
                   dest="radix", help="octal radix (same as -Ao)")
    p.add_argument("--format", choices=["berkeley", "sysv", "both"],
                   help="output format")
    p.add_argument("--common", action="store_true",
                   help="show common symbols separately")