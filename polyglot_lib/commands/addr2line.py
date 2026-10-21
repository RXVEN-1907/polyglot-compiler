"""The 'addr2line' command: convert addresses to file names and line numbers."""

import argparse
import hashlib
import json
import os
from pathlib import Path
from typing import Dict, Any, List, Optional

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError

ORDER = 180


def _get_cache_key(paths, addresses, functions, pretty, bases, demangle, filename, base):
    """Generate a cache key based on input parameters."""
    key_data = {
        "paths": sorted([str(p) for p in paths]),
        "addresses": sorted([str(a) for a in addresses]) if addresses else None,
        "functions": functions,
        "pretty": pretty,
        "bases": bases,
        "demangle": demangle,
        "filename": filename,
        "base": base,
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


def _parse_addr2line_output(lines):
    """Parse addr2line output into structured format."""
    locations = []

    for line in lines:
        if line.strip():
            # Each line represents the file:line for an address
            locations.append(line.strip())

    return {"locations": locations}


@command("addr2line", pass_through=True)
def cmd_addr2line(tc, args):
    """Convert addresses to file names and line numbers using addr2line with optional JSON output and caching."""
    # Handle JSON output flag
    as_json = getattr(args, "json", False)

    # If JSON output requested and we have cached results, return them
    if as_json and not getattr(args, "no_cache", False):
        cache_key = _get_cache_key(
            args.paths or [],
            getattr(args, "addresses", None),
            getattr(args, "functions", False),
            getattr(args, "pretty", False),
            getattr(args, "bases", False),
            getattr(args, "demangle", False),
            getattr(args, "filename", False),
            getattr(args, "base", False)
        )
        cached = _get_cached_result(cache_key)
        if cached is not None:
            print(json.dumps(cached, indent=2))
            return

    # Try to find the GNU 'addr2line' command first (from binutils), then fallback to 'llvm-addr2line'
    addr2line_path = tc.find_tool("addr2line")
    if addr2line_path is None:
        addr2line_path = tc.tool_or_error("llvm-addr2line", "converting addresses to file names and line numbers")

    # Build command arguments
    import subprocess
    import sys

    cmd = [str(addr2line_path)]

    # Add the executable file(s) with -e flag
    # addr2line expects one executable via -e, but we'll use the first one if multiple are provided
    if getattr(args, "paths", None) and len(args.paths) > 0:
        cmd.extend(["-e", str(args.paths[0])])

    # Add arguments based on flags
    if getattr(args, "addresses", None):
        # Addresses can be passed as separate arguments
        cmd.extend(["--addresses"])
        cmd.extend([str(addr) for addr in getattr(args, "addresses")])
    if getattr(args, "functions", False):
        cmd.append("-f")
    if getattr(args, "pretty", False):
        cmd.append("-p")
    if getattr(args, "bases", False):
        cmd.append("-b")
    if getattr(args, "demangle", False):
        cmd.append("-C")
    if getattr(args, "filename", False):
        cmd.append("-fi")
    if getattr(args, "base", False):
        cmd.append("-s")

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)

        # If JSON output requested, parse and structure the output
        if as_json:
            # Parse the output into structured format
            lines = result.stdout.strip().split('\n')
            structured_data = _parse_addr2line_output(lines)

            # Add metadata
            structured_data["_meta"] = {
                "tool": "llvm-addr2line" if "llvm-addr2line" in str(addr2line_path) else "addr2line",
                "command": " ".join(cmd),
                "count": len(structured_data["locations"])
            }

            # Cache the result
            if not getattr(args, "no_cache", False):
                cache_key = _get_cache_key(
                    args.paths or [],
                    getattr(args, "addresses", None),
                    getattr(args, "functions", False),
                    getattr(args, "pretty", False),
                    getattr(args, "bases", False),
                    getattr(args, "demangle", False),
                    getattr(args, "filename", False),
                    getattr(args, "base", False)
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
                "error": f"addr2line tool failed with return code {e.returncode}",
                "stdout": e.stdout,
                "stderr": e.stderr,
                "_meta": {
                    "tool": "llvm-addr2line" if "llvm-addr2line" in str(addr2line_path) else "addr2line",
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
        raise PolyglotError("failed to execute addr2line tool: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("addr2line", help="Convert addresses to file names and line numbers")
    p.add_argument("paths", nargs="*", help="executable file")
    # Common flags that addr2line accepts (GNU binutils and llvm-addr2line)
    p.add_argument("--addresses", metavar="ADDRESS", action="append",
                   help="specify hexadecimal addresses to convert")
    p.add_argument("-f", "--functions", action="store_true",
                   help="display function names as well as file and line numbers")
    p.add_argument("-p", "--pretty", action="store_true",
                   help="make the output easier to read for humans")
    p.add_argument("-b", "--bases", action="store_true",
                   help="display base addresses of objects containing symbols")
    p.add_argument("-C", "--demangle", action="store_true",
                   help="demangle symbol names")
    p.add_argument("-fi", "--filename", action="store_true",
                   help="display source file names and line numbers")
    p.add_argument("-s", "--base", action="store_true",
                   help="display the base address of the containing object")
    # Polyglot-specific enhancements
    p.add_argument("--json", action="store_true",
                   help="output in JSON format for programmatic use")
    p.add_argument("--no-cache", action="store_true",
                   help="disable caching of inspection results")