"""Shared utilities for binary inspection commands."""

import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path
from typing import Dict, Any, Optional, Callable, List, Union

from polyglot_lib.output import PolyglotError
from polyglot_lib.inspection_cache import InspectionCache


# Global inspection cache instance
_inspection_cache = None


def get_inspection_cache() -> InspectionCache:
    """Get or create the global inspection cache instance."""
    global _inspection_cache
    if _inspection_cache is None:
        _inspection_cache = InspectionCache()
    return _inspection_cache


def _get_inspection_cache_dir() -> Path:
    """Get the cache directory for inspection results."""
    return Path.home() / ".polyglot-cache" / "inspection"


def _get_cache_key(base_data: Dict[str, Any]) -> str:
    """Generate a cache key based on input data."""
    key_str = json.dumps(base_data, sort_keys=True)
    return hashlib.sha256(key_str.encode()).hexdigest()[:16]


def _get_cached_result(cache_key: str) -> Optional[Dict[str, Any]]:
    """Retrieve cached result if available and valid."""
    return get_inspection_cache().get("shared", cache_key=cache_key)


def _cache_result(cache_key: str, data: Dict[str, Any]) -> None:
    """Cache the inspection result."""
    get_inspection_cache().set("shared", data, cache_key=cache_key)


def _find_inspection_tool(tc, preferred_tool: str, fallback_tool: str, operation_desc: str) -> Path:
    """Find an inspection tool, trying GNU tool first, then LLVM fallback."""
    tool_path = tc.find_tool(preferred_tool)
    if tool_path is None:
        tool_path = tc.tool_or_error(fallback_tool, operation_desc)
    return tool_path


def run_inspection_command(
    tc,
    args,
    preferred_tool: str,
    fallback_tool: str,
    operation_desc: str,
    tool_args_func: Callable[[Any], List[str]],
    parse_func: Optional[Callable[[List[str]], Dict[str, Any]]] = None,
    cache_key_func: Optional[Callable[[Any], Optional[str]]] = None,
    use_tool_cache: bool = True
) -> None:
    """Run a binary inspection command with shared infrastructure.

    This function handles the common pattern for inspection commands:
    - JSON output handling
    - Caching
    - Tool discovery (GNU -> LLVM fallback)
    - Subprocess execution
    - Result parsing and metadata addition
    - Error handling

    Args:
        tc: Toolchain instance
        args: Parsed command line arguments
        preferred_tool: GNU tool name (e.g., "nm")
        fallback_tool: LLVM tool name (e.g., "llvm-nm")
        operation_desc: Description of operation for error messages
        tool_args_func: Function that converts args to tool command line arguments
        parse_func: Function to parse tool output lines into structured data (optional)
        cache_key_func: Function that generates cache key from args (optional)
        use_tool_cache: Whether to use caching for this command (default: True)
    """
    # Handle JSON output flag
    as_json = getattr(args, "json", False)
    no_cache = getattr(args, "no_cache", False)

    # Generate cache key if needed
    cache_key = None
    if cache_key_func is not None:
        cache_key = cache_key_func(args)

        # Check for cached JSON result
        if as_json and not no_cache and use_tool_cache:
            cached = get_inspection_cache().get(
                "shared",
                cache_key=cache_key
            )
            if cached is not None:
                print(json.dumps(cached, indent=2))
                return

    # Find the tool
    tool_path = _find_inspection_tool(tc, preferred_tool, fallback_tool, operation_desc)

    # Build tool command
    tool_cmd = [str(tool_path)] + tool_args_func(args)

    # Run the tool
    try:
        result = subprocess.run(tool_cmd, capture_output=True, text=True, check=True)

        # Handle JSON output
        if as_json:
            # Parse and structure the output
            if parse_func:
                # Parse the output into structured format
                lines = result.stdout.strip().split('\n')
                structured_data = parse_func(lines)
            else:
                # Use raw output
                structured_data = {"raw_output": result.stdout}

            # Add metadata
            structured_data["_meta"] = {
                "tool": "llvm-" + preferred_tool if "llvm-" in str(tool_path) else preferred_tool,
                "command": " ".join(tool_cmd),
                "return_code": result.returncode
            }

            # Cache the result
            if cache_key is not None and not no_cache and use_tool_cache:
                get_inspection_cache().set(
                    "shared",
                    structured_data,
                    cache_key=cache_key
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
                "error": f"{preferred_tool} tool failed with return code {e.returncode}",
                "stdout": e.stdout,
                "stderr": e.stderr,
                "_meta": {
                    "tool": "llvm-" + preferred_tool if "llvm-" in str(tool_path) else preferred_tool,
                    "command": " ".join(tool_cmd),
                    "return_code": e.returncode
                }
            }
            print(json.dumps(error_data, indent=2))
        else:
            sys.stderr.write(e.stdout or "")
            sys.stderr.write(e.stderr or "")
        sys.exit(e.returncode)
    except OSError as e:
        raise PolyglotError(f"failed to execute {preferred_tool} tool: {e}")