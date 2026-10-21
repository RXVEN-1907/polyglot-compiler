"""The 'c++filt' command: demangle C++ and Rust symbols."""

import argparse
import hashlib
import json
import os
from pathlib import Path
from typing import Dict, Any, List, Optional

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError

ORDER = 185


def _get_cache_key(paths, symbols, strip_underscore, no_verbose, recurse_limit, types, format):
    """Generate a cache key based on input parameters."""
    key_data = {
        "paths": sorted([str(p) for p in paths]),
        "symbols": sorted([str(s) for s in symbols]) if symbols else None,
        "strip_underscore": strip_underscore,
        "no_verbose": no_verbose,
        "recurse_limit": recurse_limit,
        "types": types,
        "format": format,
    }
    key_str = json.dumps(key_data, sort_keys=True)
    return hashlib.sha256(key_str.encode()).hexdigest()[:16]


def _get_cached_result(cache_key):
    """Retrieve cached result if available and valid."""
    cache_dir = Path.home() / ".polyglot-cache" / "inspection"
    cache_file = cache_dir / f"{cache_key}.json"

    if cache_file.exists():
        try:
            with open(cache_file, 'r') as f:
                return json.load(f)
        except (json.JSONDecodeError, OSError):
            pass
    return None


def _cache_result(cache_key, data):
    """Cache the inspection result."""
    cache_dir = Path.home() / ".polyglot-cache" / "inspection"
    cache_dir.mkdir(parents=True, exist_ok=True)
    cache_file = cache_dir / f"{cache_key}.json"

    try:
        with open(cache_file, 'w') as f:
            json.dump(data, f, indent=2)
    except OSError:
        pass  # Fail silently for caching


def _parse_cxxfilt_output(lines):
    """Parse c++filt output into structured format."""
    demangled = []

    for line in lines:
        if line.strip():
            demangled.append(line.strip())

    return {"demangled": demangled}


@command("c++filt", pass_through=True)
def cmd_cxxfilt(tc, args):
    """Demangle C++ and Rust symbols using c++filt with optional JSON output and caching."""
    # Handle JSON output flag
    as_json = getattr(args, "json", False)

    # If JSON output requested and we have cached results, return them
    if as_json and not getattr(args, "no_cache", False):
        cache_key = _get_cache_key(
            args.paths or [],
            getattr(args, "symbols", None),
            getattr(args, "strip_underscore", False),
            getattr(args, "no_verbose", False),
            getattr(args, "recurse_limit", False),
            getattr(args, "types", False),
            getattr(args, "format", None)
        )
        cached = _get_cached_result(cache_key)
        if cached is not None:
            print(json.dumps(cached, indent=2))
            return

    # Try to find the GNU 'c++filt' command first (from binutils), then fallback to 'llvm-c++filt'
    cxxfilt_path = tc.find_tool("c++filt")
    if cxxfilt_path is None:
        cxxfilt_path = tc.tool_or_error("llvm-c++filt", "demangling C++ and Rust symbols")

    # Build command arguments
    import subprocess
    import sys

    cmd = [str(cxxfilt_path)]

    # Add arguments based on flags
    if getattr(args, "symbols", None):
        # Symbols can be passed as separate arguments or stdin
        # For simplicity, we'll handle them as direct arguments
        cmd.extend([str(sym) for sym in getattr(args, "symbols")])
    if getattr(args, "strip_underscore", False):
        cmd.append("-_")  # Strip leading underscore
    elif not getattr(args, "strip_underscore", True):  # Default is to not strip
        cmd.append("-n")  # Do not strip leading underscore
    if getattr(args, "no_verbose", False):
        cmd.append("-i")
    if getattr(args, "recurse_limit", False):
        cmd.append("-R")
    elif not getattr(args, "recurse_limit", True):  # Default is to have limit
        cmd.append("-r")
    if getattr(args, "types", False):
        cmd.append("-t")
    if getattr(args, "format", None):
        cmd.extend(["-s", getattr(args, "format")])

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)

        # If JSON output requested, parse and structure the output
        if as_json:
            # Parse the output into structured format
            lines = result.stdout.strip().split('\n')
            structured_data = _parse_cxxfilt_output(lines)

            # Add metadata
            structured_data["_meta"] = {
                "tool": "llvm-c++filt" if "llvm-c++filt" in str(cxxfilt_path) else "c++filt",
                "command": " ".join(cmd),
                "count": len(structed_data["demangled"])
            }

            # Cache the result
            if not getattr(args, "no_cache", False):
                cache_key = _get_cache_key(
                    args.paths or [],
                    getattr(args, "symbols", None),
                    getattr(args, "strip_underscore", False),
                    getattr(args, "no_verbose", False),
                    getattr(args, "recurse_limit", False),
                    getattr(args, "types", False),
                    getattr(args, "format", None)
                )
                _cache_result(cache_key, structured_data)

            print(json.dumps(structured_data, indent=2))
        else:
            # Traditional output
            sys.stdout.write(result.stdout)
            if result.stderr:
                sys.stderr.write(result.stderr)
            sys.exit(result.returncode)

    except subprocess.CalledProcessError as e:
        if as_json:
            error_data = {
                "error": f"c++filt tool failed with return code {e.returncode}",
                "stdout": e.stdout,
                "stderr": e.stderr,
                "_meta": {
                    "tool": "llvm-c++filt" if "llvm-c++filt" in str(cxxfilt_path) else "c++filt",
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
        raise PolyglotError("failed to execute c++filt tool: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("c++filt", help="Demangle C++ and Rust symbols")
    p.add_argument("paths", nargs="*", help="optional object files to read symbols from")
    # Common flags that c++filt accepts (GNU binutils and llvm-c++filt)
    p.add_argument("--symbols", metavar="SYMBOL", action="append",
                   help="symbols to demangle (if not provided, read from stdin)")
    p.add_argument("-_", "--strip-underscore", action="store_true",
                   help="ignore first leading underscore")
    p.add_argument("-n", "--no-strip-underscore", action="store_true",
                   help="do not ignore a leading underscore (default)")
    p.add_argument("-p", "--no-params", action="store_true",
                   help="do not display function arguments")
    p.add_argument("-i", "--no-verbose", action="store_true",
                   help="do not show implementation details")
    p.add_argument("-R", "--recurse-limit", action="store_true",
                   help="enable recursion limit (default)")
    p.add_argument("-r", "--no-recurse-limit", action="store_true",
                   help="disable recursion limit")
    p.add_argument("-t", "--types", action="store_true",
                   help="also attempt to demangle type encodings")
    p.add_argument("-s", "--format", choices=["none", "auto", "gnu-v3", "java", "gnat", "dlang", "rust"],
                   help="demangle using the specified format")
    # Polyglot-specific enhancements
    p.add_argument("--json", action="store_true",
                   help="output in JSON format for programmatic use")
    p.add_argument("--no-cache", action="store_true",
                   help="disable caching of inspection results")