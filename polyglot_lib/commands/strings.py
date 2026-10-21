"""The 'strings' command: display printable strings from files."""

import argparse
import hashlib
import json
import os
from pathlib import Path
from typing import Dict, Any, List, Optional

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError

ORDER = 175


def _get_cache_key(paths, all, bytes, radix, format, filename, help_flag, unwarn):
    """Generate a cache key based on input parameters."""
    key_data = {
        "paths": sorted([str(p) for p in paths]),
        "all": all,
        "bytes": bytes,
        "radix": radix,
        "format": format,
        "filename": filename,
        "help": help_flag,
        "unwarn": unwarn,
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


def _parse_strings_output(lines, radix):
    """Parse strings output into structured format."""
    strings = []

    for line in lines:
        if line.strip():
            parts = line.split()
            if len(parts) >= 2:
                # Try to parse as: address string or just string
                try:
                    addr = parts[0]
                    # Check if first part looks like an address in the specified radix
                    if radix == "d":
                        int(addr)  # decimal
                    elif radix == "o":
                        int(addr, 8)  # octal
                    elif radix == "x":
                        int(addr, 16)  # hexadecimal
                    # If it parses as an integer, treat it as address
                    string_val = ' '.join(parts[1:])
                    strings.append({
                        "address": addr,
                        "string": string_val
                    })
                except ValueError:
                    # First part is not an address, whole line is the string
                    strings.append({
                        "address": None,
                        "string": line
                    })
            else:
                # Only one part - just the string
                strings.append({
                    "address": None,
                    "string": line
                })

    return {"strings": strings}


@command("strings", pass_through=True)
def cmd_strings(tc, args):
    """Display printable strings from files using strings with optional JSON output and caching."""
    # Handle JSON output flag
    as_json = getattr(args, "json", False)

    # If JSON output requested and we have cached results, return them
    if as_json and not getattr(args, "no_cache", False):
        cache_key = _get_cache_key(
            args.paths or [],
            getattr(args, "all", False),
            getattr(args, "bytes", None),
            getattr(args, "radix", None),
            getattr(args, "format", None),
            getattr(args, "filename", False),
            getattr(args, "help", False),
            getattr(args, "unwarn", False)
        )
        cached = _get_cached_result(cache_key)
        if cached is not None:
            print(json.dumps(cached, indent=2))
            return

    # Try to find the GNU 'strings' command first (from binutils), then fallback to 'llvm-strings'
    strings_path = tc.find_tool("strings")
    if strings_path is None:
        strings_path = tc.tool_or_error("llvm-strings", "displaying printable strings")

    # Build command arguments
    import subprocess
    import sys

    cmd = [str(strings_path)]

    # Add arguments based on flags
    if getattr(args, "all", False):
        cmd.append("-a")
    if getattr(args, "bytes") is not None:
        cmd.extend(["-n", str(getattr(args, "bytes"))])
    if getattr(args, "radix") is not None:
        cmd.extend(["-t", getattr(args, "radix")])
    if getattr(args, "format") is not None:
        # Note: format flag handling varies between implementations
        # For simplicity, we'll skip it here or handle in a more complex way
        pass
    if getattr(args, "filename", False):
        cmd.append("-f")
    # Skip custom --help as argparse provides it automatically
    if getattr(args, "unwarn", False):
        cmd.append("--unwarn-disable")

    # Add file paths
    cmd.extend(list(args.paths or []))

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)

        # If JSON output requested, parse and structure the output
        if as_json:
            # Parse the output into structured format
            lines = result.stdout.strip().split('\n')
            radix = getattr(args, "radix", None) or "d"  # default to decimal
            structured_data = _parse_strings_output(lines, radix)

            # Add metadata
            structured_data["_meta"] = {
                "tool": "llvm-strings" if "llvm-strings" in str(strings_path) else "strings",
                "command": " ".join(cmd),
                "count": len(structured_data["strings"])
            }

            # Cache the result
            if not getattr(args, "no_cache", False):
                cache_key = _get_cache_key(
                    args.paths or [],
                    getattr(args, "all", False),
                    getattr(args, "bytes", None),
                    getattr(args, "radix", None),
                    getattr(args, "format", None),
                    getattr(args, "filename", False),
                    getattr(args, "help", False),
                    getattr(args, "unwarn", False)
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
                "error": f"strings tool failed with return code {e.returncode}",
                "stdout": e.stdout,
                "stderr": e.stderr,
                "_meta": {
                    "tool": "llvm-strings" if "llvm-strings" in str(strings_path) else "strings",
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
        raise PolyglotError("failed to execute strings tool: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("strings", help="Display printable strings from files")
    p.add_argument("paths", nargs="*", help="files to search")
    # Common flags that strings accepts (GNU binutils and llvm-strings)
    p.add_argument("-a", "--all", action="store_true",
                   help="scan entire files, not just data sections")
    p.add_argument("-n", "--bytes", metavar="NUMBER",
                   help="separate printing of sequences that are less than NUMBER")
    p.add_argument("-t", "--radix", choices=["d", "o", "x"],
                   help="print offsets in radix d (decimal), o (octal), or x (hexadecimal)")
    p.add_argument("--format", choices=["bsd", "sysv", "posix"],
                   help="output format (not fully implemented)")
    p.add_argument("-f", "--filename", action="store_true",
                   help="print filename before each string")
    # Note: Do not add custom --help argument as argparse provides it automatically
    p.add_argument("--unwarn", action="store_true",
                   help="disable warnings about overflowed buffer lengths")
    # Polyglot-specific enhancements
    p.add_argument("--json", action="store_true",
                   help="output in JSON format for programmatic use")
    p.add_argument("--no-cache", action="store_true",
                   help="disable caching of inspection results")