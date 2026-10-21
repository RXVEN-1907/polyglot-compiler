"""The 'nm' command: list symbols in object files and executables."""

import argparse
import json
from pathlib import Path
from typing import Dict, Any, List, Optional

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError
from polyglot_lib.inspection_cache import get_inspection_cache
from polyglot_lib.commands.inspection_utils import _parse_nm_output

ORDER = 150


@command("nm", pass_through=True)
def cmd_nm(tc, args):
    """List symbols using nm or llvm-nm with optional JSON output and caching."""
    # Handle JSON output flag
    as_json = getattr(args, "json", False)
    no_cache = getattr(args, "no_cache", False)

    # Check for cached JSON result
    if as_json and not no_cache:
        cache_data = get_inspection_cache().get(
            "nm",
            paths=getattr(args, "paths", []),
            debug_syms=getattr(args, "debug_syms", False),
            extern_only=getattr(args, "extern_only", False),
            undefined_only=getattr(args, "undefined_only", False),
            defined_only=getattr(args, "defined_only", False),
            posix=getattr(args, "posix", False),
            format=getattr(args, "format", None),
            radix=getattr(args, "radix", None),
            no_sort=getattr(args, "no_sort", False),
        )
        if cache_data is not None:
            print(json.dumps(cache_data, indent=2))
            return

    # Select tool based on compiler backend to respect user's toolchain choice
    # When using clang, prefer llvm-nm; when using gcc, prefer GNU nm
    from polyglot_lib.toolchain import Toolchain
    if isinstance(tc, Toolchain) and tc.kind == "clang":
        # Prefer llvm-nm for clang toolchain
        nm_path = tc.find_tool("llvm-nm")
        if nm_path is None:
            nm_path = tc.find_tool("nm")
    else:
        # Prefer GNU nm for gcc toolchain (or when kind is unknown/None)
        nm_path = tc.find_tool("nm")
        if nm_path is None:
            nm_path = tc.find_tool("llvm-nm")

    if nm_path is None:
        nm_path = tc.tool_or_error("llvm-nm", "listing symbols")

    # Build nm command
    cmd = [str(nm_path)]

    # Add common flags
    if getattr(args, "debug_syms", False):
        cmd.append("-a")
    if getattr(args, "extern_only", False):
        cmd.append("-g")
    if getattr(args, "undefined_only", False):
        cmd.append("-u")
    if getattr(args, "defined_only", False):
        cmd.append("--defined-only")
    if getattr(args, "posix", False):
        cmd.append("-P")
    if getattr(args, "format"):
        cmd.extend(["-f", getattr(args, "format")])
    if getattr(args, "no_sort", False):
        cmd.append("-p")
    if getattr(args, "radix"):
        cmd.extend(["--radix", getattr(args, "radix")])

    # Add file paths
    cmd.extend([str(p) for p in (args.paths or [])])

    # Run the tool
    import subprocess
    import sys

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)

        # Handle JSON output
        if as_json:
            # Parse the output into structured format
            structured_data = _parse_nm_output(result.stdout.strip())

            # Add metadata
            structured_data["_meta"] = {
                "tool": "llvm-nm" if "llvm-nm" in str(nm_path) else "nm",
                "command": " ".join(cmd),
                "count": len(structured_data["symbols"])
            }

            # Cache the result
            if not no_cache:
                get_inspection_cache().set(
                    "nm",
                    structured_data,
                    paths=getattr(args, "paths", []),
                    debug_syms=getattr(args, "debug_syms", False),
                    extern_only=getattr(args, "extern_only", False),
                    undefined_only=getattr(args, "undefined_only", False),
                    defined_only=getattr(args, "defined_only", False),
                    posix=getattr(args, "posix", False),
                    format=getattr(args, "format", None),
                    radix=getattr(args, "radix", None),
                    no_sort=getattr(args, "no_sort", False),
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
                "error": f"nm tool failed with return code {e.returncode}",
                "stdout": e.stdout,
                "stderr": e.stderr,
                "_meta": {
                    "tool": "llvm-nm" if "llvm-nm" in str(nm_path) else "nm",
                    "command": " ".join(cmd),
                    "return_code": e.returncode
                }
            }
            print(json.dumps(error_data, indent=2))
        else:
            sys.stderr.write(e.stdout or "")
            sys.stderr.write(e.stderr or "")
        sys.exit(e.returncode)
    except OSError as e:
        raise PolyglotError("failed to execute nm tool: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("nm", help="List symbols in object files")
    p.add_argument("paths", nargs="*", help="object files or executables")
    # Common flags that nm accepts (GNU binutils and llvm-nm)
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
    # Polyglot-specific enhancements
    p.add_argument("--json", action="store_true",
                   help="output in JSON format for programmatic use")
    p.add_argument("--no-cache", action="store_true",
                   help="disable caching of inspection results")