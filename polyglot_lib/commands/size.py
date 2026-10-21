"""The 'size' command: show section sizes of object files."""

import argparse
import json
from pathlib import Path
from typing import Dict, Any, List, Optional

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError
from polyglot_lib.inspection_cache import get_inspection_cache
from polyglot_lib.commands.inspection_utils import _parse_size_output

ORDER = 155


@command("size", pass_through=True)
def cmd_size(tc, args):
    """Show section sizes using size or llvm-size with optional JSON output and caching."""
    # Handle JSON output flag
    as_json = getattr(args, "json", False)
    no_cache = getattr(args, "no_cache", False)

    # Check for cached JSON result
    if as_json and not no_cache:
        cache_data = get_inspection_cache().get(
            "size",
            paths=getattr(args, "paths", []),
            format=getattr(args, "format", None),
            radix=getattr(args, "radix", None),
            berkeley=getattr(args, "berkeley", False),
            common=getattr(args, "common", False),
        )
        if cache_data is not None:
            print(json.dumps(cache_data, indent=2))
            return

    # Select size tool based on compiler backend to respect user's toolchain choice
    # When using clang, prefer llvm-size; when using gcc, prefer GNU size
    from polyglot_lib.toolchain import Toolchain
    if isinstance(tc, Toolchain) and tc.kind == "clang":
        # Prefer llvm-size for clang toolchain
        size_path = tc.find_tool("llvm-size")
        if size_path is None:
            size_path = tc.find_tool("size")
    else:
        # Prefer GNU size for gcc toolchain (or when kind is unknown/None)
        size_path = tc.find_tool("size")
        if size_path is None:
            size_path = tc.find_tool("llvm-size")

    if size_path is None:
        size_path = tc.tool_or_error("llvm-size" if isinstance(tc, Toolchain) and tc.kind == "clang" else "size", "showing section sizes")

    # Build size command
    cmd = [str(size_path)]

    # Add common flags
    if getattr(args, "radix"):
        cmd.extend(["--radix", getattr(args, "radix")])
    if getattr(args, "berkeley", False):
        cmd.append("-B")
    if getattr(args, "decimal", False):
        cmd.append("-d")
    if getattr(args, "hexadecimal", False):
        cmd.append("-x")
    if getattr(args, "octal", False):
        cmd.append("-o")
    if getattr(args, "format"):
        cmd.extend(["--format", getattr(args, "format")])
    if getattr(args, "common", False):
        cmd.append("--common")

    # Add file paths
    cmd.extend([str(p) for p in (args.paths or [])])

    # Run the tool
    import subprocess
    import sys

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)

        # Handle JSON output
        if as_json:
            # Parse the traditional output into structured format
            structured_data = _parse_size_output(result.stdout.strip())

            # Cache the result
            if not no_cache:
                get_inspection_cache().set(
                    "size",
                    structured_data,
                    paths=getattr(args, "paths", []),
                    format=getattr(args, "format", None),
                    radix=getattr(args, "radix", None),
                    berkeley=getattr(args, "berkeley", False),
                    common=getattr(args, "common", False),
                )

            print(json.dumps(structured_data, indent=2))
        else:
            # Traditional output
            sys.stdout.write(result.stdout)
            if result.stderr:
                sys.stderr.write(result.stderr)
            sys.exit(result.returncode)

    except subprocess.CalledProcessError as e:
        # Handle JSON error output
        if as_json:
            error_data = {
                "error": f"size tool failed with return code {e.returncode}",
                "stdout": e.stdout,
                "stderr": e.stderr
            }
            print(json.dumps(error_data, indent=2))
        else:
            sys.stderr.write(e.stdout or "")
            sys.stderr.write(e.stderr or "")
        sys.exit(e.returncode)
    except OSError as e:
        raise PolyglotError("failed to execute size tool: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("size", help="Show section sizes of object files")
    p.add_argument("paths", nargs="*", help="object files or executables")
    # Common flags that size accepts (GNU binutils and llvm-size)
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
    # Polyglot-specific enhancements
    p.add_argument("--json", action="store_true",
                   help="output in JSON format for programmatic use")
    p.add_argument("--no-cache", action="store_true",
                   help="disable caching of inspection results")