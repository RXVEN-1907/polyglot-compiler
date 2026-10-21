"""The 'strip' command: strip symbols from object files."""

import argparse
import hashlib
import json
import os
from pathlib import Path
from typing import Dict, Any, List, Optional

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError
from polyglot_lib.inspection_cache import get_inspection_cache

ORDER = 170


def _get_cache_key(args) -> Optional[str]:
    """Generate a cache key based on strip arguments."""
    # Note: strip command modifies files, so caching is less applicable for actual operations
    # We'll still generate a key for potential dry-run or info purposes
    if not hasattr(args, 'paths') or not args.paths:
        # No paths means we can't meaningfully cache
        return None

    key_data = {
        "paths": sorted([str(p) for p in args.paths]),
        "strip_debug": getattr(args, "strip_debug", False),
        "keep_symbol": getattr(args, "keep_symbol", None),
        "strip_symbol": getattr(args, "strip_symbol", None),
        "output": getattr(args, "output", None),
        "preserve_dates": getattr(args, "preserve_dates", False),
        "remove_section": getattr(args, "remove_section", None),
        "strip_all": getattr(args, "strip_all", False),
        "strip_unneeded": getattr(args, "strip_unneeded", False),
        "wildcard": getattr(args, "wildcard", False),
        "discard_all": getattr(args, "discard_all", False),
        "discard_locals": getattr(args, "discard_locals", False),
        # For actual strip operations (not dry-run), we don't cache because files are modified
        # But we include a flag to distinguish dry-run from actual operations for caching purposes
        "dry_run": getattr(args, "dry_run", False),
    }
    key_str = json.dumps(key_data, sort_keys=True)
    return hashlib.sha256(key_str.encode()).hexdigest()[:16]


def _parse_strip_output(lines: List[str]) -> Dict[str, Any]:
    """Parse strip output into structured format (simplified)."""
    # Strip output is usually minimal, but we can capture what it outputs
    data = {
        "format": "parsed",
        "stripped_files": [],
        "messages": []
    }

    for line in lines:
        if line.strip():
            # Try to parse common strip output formats
            if ": " in line and ("stripped" in line.lower() or "removed" in line.lower()):
                parts = line.split(": ", 1)
                if len(parts) == 2:
                    file_part, action_part = parts
                    data["stripped_files"].append({
                        "file": file_part.strip(),
                        "action": action_part.strip()
                    })
                else:
                    data["messages"].append(line)
            else:
                data["messages"].append(line)

    return data


@command("strip", pass_through=True)
def cmd_strip(tc, args):
    """Strip symbols using strip or llvm-strip with optional JSON output."""
    # Handle JSON output flag
    as_json = getattr(args, "json", False)
    no_cache = getattr(args, "no_cache", False)
    dry_run = getattr(args, "dry_run", False)

    # For strip operations, caching is primarily useful for dry-run since actual strip modifies files
    # But we can still cache dry-run results
    if as_json and not no_cache and dry_run:
        cache_key = _get_cache_key(args)
        if cache_key is not None:
            cached = get_inspection_cache().get(
                "strip",
                paths=getattr(args, "paths", []),
                strip_debug=getattr(args, "strip_debug", False),
                keep_symbol=getattr(args, "keep_symbol", None),
                strip_symbol=getattr(args, "strip_symbol", None),
                output=getattr(args, "output", None),
                preserve_dates=getattr(args, "preserve_dates", False),
                remove_section=getattr(args, "remove_section", None),
                strip_all=getattr(args, "strip_all", False),
                strip_unneeded=getattr(args, "strip_unneeded", False),
                wildcard=getattr(args, "wildcard", False),
                discard_all=getattr(args, "discard_all", False),
                discard_locals=getattr(args, "discard_locals", False),
                dry_run=True,
            )
            if cached is not None:
                print(json.dumps(cached, indent=2))
                return

    # Try to find the GNU 'strip' command first (from binutils), then fallback to 'llvm-strip'
    strip_path = tc.find_tool("strip")
    if strip_path is None:
        strip_path = tc.tool_or_error("llvm-strip", "stripping symbols")

    # Forward all arguments to the strip tool
    import subprocess
    import sys

    cmd = [str(strip_path)] + list(args.paths or [])
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)

        # Handle JSON output (available for both dry-run and actual operations)
        if as_json:
            # Parse the output into structured format
            lines = result.stdout.strip().split('\n')
            structured_data = _parse_strip_output(lines)

            # Add metadata
            structured_data["_meta"] = {
                "tool": "llvm-strip" if "llvm-strip" in str(strip_path) else "strip",
                "command": " ".join(cmd),
                "return_code": result.returncode
            }

            # Cache the result (primarily useful for dry-run)
            if not no_cache and dry_run:
                cache_key = _get_cache_key(args)
                if cache_key is not None:
                    get_inspection_cache().set(
                        "strip",
                        structured_data,
                        paths=getattr(args, "paths", []),
                        strip_debug=getattr(args, "strip_debug", False),
                        keep_symbol=getattr(args, "keep_symbol", None),
                        strip_symbol=getattr(args, "strip_symbol", None),
                        output=getattr(args, "output", None),
                        preserve_dates=getattr(args, "preserve_dates", False),
                        remove_section=getattr(args, "remove_section", None),
                        strip_all=getattr(args, "strip_all", False),
                        strip_unneeded=getattr(args, "strip_unneeded", False),
                        wildcard=getattr(args, "wildcard", False),
                        discard_all=getattr(args, "discard_all", False),
                        discard_locals=getattr(args, "discard_locals", False),
                        dry_run=True,
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
                "error": f"strip tool failed with return code {e.returncode}",
                "stdout": e.stdout,
                "stderr": e.stderr,
                "_meta": {
                    "tool": "llvm-strip" if "llvm-strip" in str(strip_path) else "strip",
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
        raise PolyglotError("failed to execute strip tool: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("strip", help="Strip symbols from object files")
    p.add_argument("paths", nargs="+", help="object files or executables")
    # Common flags that strip accepts (GNU binutils and llvm-strip)
    p.add_argument("-F", action="store_true",
                   help="display strip help")
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
                   help="discard all non-global symbols")
    p.add_argument("-X", "--discard-locals", action="store_true",
                   help="discard compiler-generated local symbols")
    # Polyglot-specific enhancements
    p.add_argument("--json", action="store_true",
                   help="output operation results in JSON format (useful with --dry-run)")
    p.add_argument("--dry-run", action="store_true",
                   help="show what would be done without actually stripping")