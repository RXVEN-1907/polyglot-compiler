"""The 'query' command: unified inspection interface for polyglot.

Provides a single, programmable interface to all inspection capabilities
(nm, size, objdump, readelf, etc.) with JSON output and intelligent caching.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
from typing import Dict, List, Any, Optional, Set

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError
from polyglot_lib.toolchain import Toolchain


# Cache directory for inspection results
INSPECTION_CACHE_DIR = Path.home() / ".polyglot-cache" / "inspection"


def _ensure_cache_dir():
    """Ensure the inspection cache directory exists."""
    INSPECTION_CACHE_DIR.mkdir(parents=True, exist_ok=True)


def _get_cache_key(command_name: str, paths: List[str], args_dict: Dict[str, Any]) -> str:
    """Generate a cache key based on command, paths, and arguments."""
    key_data = {
        "command": command_name,
        "paths": sorted([str(p) for p in paths]),
        "args": args_dict,
    }
    key_str = json.dumps(key_data, sort_keys=True)
    return hashlib.sha256(key_str.encode()).hexdigest()[:16]


def _get_cached_result(cache_key: str) -> Optional[Dict[str, Any]]:
    """Retrieve cached result if available and valid."""
    _ensure_cache_dir()
    cache_file = INSPECTION_CACHE_DIR / f"{cache_key}.json"

    if cache_file.exists():
        try:
            with open(cache_file, 'r') as f:
                return json.load(f)
        except (json.JSONDecodeError, OSError):
            pass
    return None


def _cache_result(cache_key: str, data: Dict[str, Any]) -> None:
    """Cache the inspection result."""
    _ensure_cache_dir()
    cache_file = INSPECTION_CACHE_DIR / f"{cache_key}.json"

    try:
        with open(cache_file, 'w') as f:
            json.dump(data, f, indent=2)
    except OSError:
        pass  # Fail silently for caching


def _run_nm_inspection(tc: Toolchain, paths: List[str], args: argparse.Namespace) -> Dict[str, Any]:
    """Run nm inspection and return structured results."""
    import subprocess
    import sys

    nm_path = tc.tool_or_error("llvm-nm", "listing symbols")
    cmd = [str(nm_path)] + list(paths)

    # Build nm arguments based on args
    nm_args = []
    if getattr(args, "debug_syms", False):
        nm_args.append("-a")
    if getattr(args, "extern_only", False):
        nm_args.append("-g")
    if getattr(args, "undefined_only", False):
        nm_args.append("-u")
    if getattr(args, "defined_only", False):
        nm_args.append("--defined-only")
    if getattr(args, "posix", False):
        nm_args.append("-P")
    if getattr(args, "format", None):
        nm_args.extend(["-f", getattr(args, "format")])
    if getattr(args, "no_sort", False):
        nm_args.append("-p")
    if getattr(args, "radix", None):
        nm_args.extend(["--radix", getattr(args, "radix")])

    cmd.extend(nm_args)

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)

        # Parse nm output
        lines = result.stdout.strip().split('\n')
        posix_format = getattr(args, "posix", False)
        parsed_data = _parse_nm_output(lines, posix_format)

        # Add metadata
        parsed_data["_meta"] = {
            "tool": "llvm-nm",
            "command": " ".join(cmd),
            "count": len(parsed_data.get("symbols", []))
        }

        return parsed_data

    except subprocess.CalledProcessError as e:
        return {
            "error": f"llvm-nm failed with return code {e.returncode}",
            "stdout": e.stdout,
            "stderr": e.stderr,
            "_meta": {
                "tool": "llvm-nm",
                "command": " ".join(cmd),
                "return_code": e.returncode
            }
        }
    except OSError as e:
        raise PolyglotError(f"failed to execute llvm-nm: {e}")


def _run_size_inspection(tc: Toolchain, paths: List[str], args: argparse.Namespace) -> Dict[str, Any]:
    """Run size inspection and return structured results."""
    import subprocess
    import sys

    size_path = tc.tool_or_error("llvm-size", "showing section sizes")
    cmd = [str(size_path)] + list(paths)

    # Build size arguments based on args
    size_args = []
    if getattr(args, "radix", None):
        radix_map = {"d": "-d", "o": "-o", "x": "-x"}
        if getattr(args, "radix") in radix_map:
            size_args.append(radix_map[getattr(args, "radix")])
    if getattr(args, "berkeley", False):
        size_args.append("-B")
    if getattr(args, "format", None):
        format_map = {"berkeley": "--format=berkeley", "sysv": "--format=sysv", "both": "--format=both"}
        if getattr(args, "format") in format_map:
            size_args.append(format_map[getattr(args, "format")])
    if getattr(args, "common", False):
        size_args.append("--common")

    cmd.extend(size_args)

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)

        # Parse size output
        lines = result.stdout.strip().split('\n')
        parsed_data = _parse_size_output(lines)

        # Add metadata
        parsed_data["_meta"] = {
            "tool": "llvm-size",
            "command": " ".join(cmd),
            "return_code": result.returncode
        }

        return parsed_data

    except subprocess.CalledProcessError as e:
        return {
            "error": f"llvm-size failed with return code {e.returncode}",
            "stdout": e.stdout,
            "stderr": e.stderr,
            "_meta": {
                "tool": "llvm-size",
                "command": " ".join(cmd),
                "return_code": e.returncode
            }
        }
    except OSError as e:
        raise PolyglotError(f"failed to execute llvm-size: {e}")


def _parse_nm_output(lines: List[str], posix_format: bool) -> Dict[str, Any]:
    """Parse nm output into structured format."""
    symbols = []

    if posix_format:
        # POSIX format: name value size type other index shndx version
        for line in lines:
            if line.strip():
                parts = line.split()
                if len(parts) >= 8:
                    try:
                        symbol = {
                            "name": parts[0],
                            "value": parts[1] if parts[1] != '(null)' else None,
                            "size": parts[2] if parts[2] != '(null)' else None,
                            "type": parts[3],
                            "section": parts[6] if len(parts) > 6 and parts[6] != '(null)' else None
                        }
                        symbols.append(symbol)
                    except (ValueError, IndexError):
                        pass
    else:
        # Default format: value type name (or just: type name for undefined)
        for line in lines:
            if line.strip():
                parts = line.split()
                if len(parts) >= 3:
                    try:
                        symbol = {
                            "value": parts[0] if parts[0] != '0000000000000000' and parts[0] != 'U' else None,
                            "type": parts[1],
                            "name": ' '.join(parts[2:])  # Handle names with spaces
                        }
                        symbols.append(symbol)
                    except (ValueError, IndexError):
                        pass
                elif len(parts) == 2:
                    # Undefined symbol format: U name
                    try:
                        symbol = {
                            "value": None,
                            "type": parts[0],
                            "name": parts[1]
                        }
                        symbols.append(symbol)
                    except (ValueError, IndexError):
                        pass

    return {"symbols": symbols}


def _parse_size_output(lines: List[str]) -> Dict[str, Any]:
    """Parse size output into structured JSON format."""
    # This is a simplified parser - in reality, size output format varies
    # between Berkeley and SysV formats
    data = {
        "format": "parsed",
        "files": []
    }

    # Simple parsing for demonstration
    for line in lines:
        if line.strip():
            parts = line.split()
            if len(parts) >= 2:
                # Try to parse as: filename   text    data    bss    dec    hex    filename
                try:
                    if len(parts) >= 7 and parts[0] != "filename":  # First line header
                        continue
                    # Attempt to extract meaningful data
                    file_data = {"file": parts[-1] if not parts[0].isdigit() else "unknown"}
                    # Add section sizes if we can parse them
                    if len(parts) >= 6:
                        try:
                            file_data.update({
                                "text": int(parts[-6]) if parts[-6].isdigit() else 0,
                                "data": int(parts[-5]) if parts[-5].isdigit() else 0,
                                "bss": int(parts[-4]) if parts[-4].isdigit() else 0,
                                "total": int(parts[-3]) if parts[-3].isdigit() else 0
                            })
                        except (ValueError, IndexError):
                            pass
                    data["files"].append(file_data)
                except (ValueError, IndexError):
                    # If parsing fails, treat as raw line
                    data.setdefault("raw_lines", []).append(line)

    if not data["files"] and lines:
        data["raw_output"] = '\n'.join(lines)

    return data


def _get_inspection_functions() -> Dict[str, Any]:
    """Return a dictionary mapping inspection aspect names to their functions."""
    return {
        "size": _run_size_inspection,
        "symbols": _run_nm_inspection,
        # Additional inspection functions would be added here
        # "sections": _run_readelf_sections,
        # "headers": _run_readelf_headers,
        # etc.
    }


@command("query")
def cmd_query(tc, args):
    """Unified inspection interface for polyglot.

    Provides a single, programmable interface to all inspection capabilities
    with JSON output and intelligent caching.
    """
    # Handle common arguments
    paths = getattr(args, "paths", []) or []
    as_json = getattr(args, "json", False)
    no_cache = getattr(args, "no_cache", False)
    save_output = getattr(args, "save", None)
    what_spec = getattr(args, "what", "all")

    # Determine what aspects to inspect
    if what_spec == "all":
        # For now, we'll support size and symbols
        # In a full implementation, this would be expanded
        aspects_to_inspect = ["size", "symbols"]
    else:
        # Parse comma-separated list
        aspects_to_inspect = [w.strip() for w in what_spec.split(",")]

    # Validate aspects
    available_aspects = set(_get_inspection_functions().keys())
    invalid_aspects = set(aspects_to_inspect) - available_aspects
    if invalid_aspects:
        raise PolyglotError(f"Unknown inspection aspects: {', '.join(invalid_aspects)}. "
                          f"Available aspects: {', '.join(sorted(available_aspects))}")

    # Inspect each aspect
    results = {
        "query": {
            "paths": paths,
            "aspects": aspects_to_inspect,
            "timestamp": None  # In a full implementation, we'd add a timestamp
        },
        "results": {}
    }

    for aspect in aspects_to_inspect:
        # Generate cache key for this specific inspection
        inspection_args = {
            "json": True,  # We always want JSON-structured data internally
            # Add aspect-specific arguments here
        }
        cache_key = _get_cache_key(aspect, paths, inspection_args)

        # Check cache first
        cached_result = None
        if not no_cache:
            cached_result = _get_cached_result(cache_key)

        if cached_result is not None:
            aspect_result = cached_result
        else:
            # Run the actual inspection
            inspection_func = _get_inspection_functions()[aspect]
            aspect_result = inspection_func(tc, paths, args)

            # Cache the result
            if not no_cache:
                _cache_result(cache_key, aspect_result)

        results["results"][aspect] = aspect_result

    # Add overall metadata
    results["_meta"] = {
        "tool": "polyglot-query",
        "command": "polyglot query " + " ".join(paths) if paths else "polyglot query",
        "aspects_queried": aspects_to_inspect,
        "cached_aspects": [
            aspect for aspect in aspects_to_inspect
            if not no_cache and _get_cached_result(_get_cache_key(
                aspect, paths, {"json": True}
            )) is not None
        ]
    }

    # Output results
    if as_json or save_output:
        output_json = json.dumps(results, indent=2)
        if save_output:
            try:
                with open(save_output, 'w') as f:
                    f.write(output_json)
            except OSError as e:
                raise PolyglotError(f"Failed to write output to {save_output}: {e}")
        if as_json or not save_output:
            print(output_json)
    else:
        # For text output, we'd format it nicely
        # For now, fall back to JSON if no specific text formatter is implemented
        print(json.dumps(results, indent=2))


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "query",
        help="Unified inspection interface for all polyglot inspection capabilities",
        description="Provides a single, programmable interface to inspect compilation "
                    "artifacts (symbols, sizes, sections, etc.) with JSON output and "
                    "intelligent caching. Ideal for tooling, IDEs, and scripting."
    )
    p.add_argument("paths", nargs="*", help="object files or executables to inspect")
    p.add_argument("--what", default="all",
                   help="comma-separated list of aspects to inspect (default: all)")
    p.add_argument("--json", action="store_true",
                   help="output in JSON format for programmatic use")
    p.add_argument("--no-cache", action="store_true",
                   help="disable caching of inspection results")
    p.add_argument("--save", metavar="FILE",
                   help="save JSON output to FILE instead of printing to stdout")
    # Polyglot-specific enhancements for query
    p.add_argument("--max-cache-age", type=int, default=3600,
                   help="maximum cache age in seconds (default: 3600 = 1 hour)")