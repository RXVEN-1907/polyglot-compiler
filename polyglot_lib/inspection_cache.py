"""Inspection cache: a unified caching system for inspection commands.

Provides consistent caching behavior across nm, size, objdump, readelf, strip,
and diff-asm commands with automatic cache expiration and size management.
"""

import hashlib
import json
import os
import time
from pathlib import Path
from typing import Any, Dict, Optional, Tuple

from polyglot_lib.output import PolyglotError


class InspectionCache:
    """Unified cache for inspection command results."""

    def __init__(self, cache_dir: str = "~/.polyglot-cache/inspection",
                 max_size_mb: int = 100, max_age_hours: int = 24):
        """Initialize the inspection cache.

        Args:
            cache_dir: Directory to store cache files
            max_size_mb: Maximum cache size in megabytes
            max_age_hours: Maximum age of cache entries in hours
        """
        self.cache_dir = Path(cache_dir).expanduser()
        self.max_size_bytes = max_size_mb * 1024 * 1024
        self.max_age_seconds = max_age_hours * 3600
        self.cache_dir.mkdir(parents=True, exist_ok=True)

    def _get_cache_key(self, prefix: str, **kwargs) -> str:
        """Generate a cache key from the given parameters.

        Args:
            prefix: Command-specific prefix (e.g., "nm", "size")
            **kwargs: Parameters that affect the cache key

        Returns:
            A hexadecimal SHA256 hash string
        """
        # Sort kwargs for consistent key generation
        sorted_items = sorted(kwargs.items())
        key_data = {
            "prefix": prefix,
            "params": sorted_items,
        }
        key_str = json.dumps(key_data, sort_keys=True, separators=(',', ':'))
        return hashlib.sha256(key_str.encode()).hexdigest()[:16]

    def _get_cache_path(self, cache_key: str) -> Path:
        """Get the filesystem path for a cache key."""
        return self.cache_dir / f"{cache_key}.json"

    def get(self, prefix: str, **kwargs) -> Optional[Dict[str, Any]]:
        """Retrieve a cached result if available and valid.

        Args:
            prefix: Command-specific prefix
            **kwargs: Parameters used to generate the cache key

        Returns:
            Cached data as a dictionary, or None if not found/expired
        """
        cache_key = self._get_cache_key(prefix, **kwargs)
        cache_path = self._get_cache_path(cache_key)

        if not cache_path.exists():
            return None

        # Check if entry is too old
        try:
            if time.time() - cache_path.stat().st_mtime > self.max_age_seconds:
                cache_path.unlink(missing_ok=True)
                return None
        except OSError:
            return None

        # Try to load and parse the cached data
        try:
            with open(cache_path, 'r') as f:
                data = json.load(f)
                # Add cache metadata
                data["_cache_meta"] = {
                    "cached_at": cache_path.stat().st_mtime,
                    "cache_key": cache_key
                }
                return data
        except (json.JSONDecodeError, OSError):
            # Remove corrupted cache file
            cache_path.unlink(missing_ok=True)
            return None

    def set(self, prefix: str, data: Dict[str, Any], **kwargs) -> None:
        """Store a result in the cache.

        Args:
            prefix: Command-specific prefix
            data: Data to cache
            **kwargs: Parameters used to generate the cache key
        """
        # Enforce cache limits before storing
        self._enforce_limits()

        cache_key = self._get_cache_key(prefix, **kwargs)
        cache_path = self._get_cache_path(cache_key)

        try:
            # Add timestamp for cache validity tracking
            cached_data = data.copy()
            cached_data["_cached_at"] = time.time()

            with open(cache_path, 'w') as f:
                json.dump(cached_data, f, indent=2, sort_keys=True)
        except OSError:
            # Fail silently for caching - cache failures shouldn't break functionality
            pass

    def _enforce_limits(self) -> None:
        """Enforce cache size and age limits by removing old entries."""
        try:
            # Get all cache files with their metadata
            cache_files = []
            for cache_file in self.cache_dir.glob("*.json"):
                try:
                    stat = cache_file.stat()
                    cache_files.append({
                        "path": cache_file,
                        "mtime": stat.st_mtime,
                        "size": stat.st_size
                    })
                except OSError:
                    # Remove unreadable files
                    cache_file.unlink(missing_ok=True)

            # Sort by modification time (oldest first)
            cache_files.sort(key=lambda x: x["mtime"])

            # Remove files that are too old
            current_time = time.time()
            fresh_files = []
            total_size = 0

            for cache_info in cache_files:
                age = current_time - cache_info["mtime"]
                if age > self.max_age_seconds:
                    # Remove old file
                    cache_info["path"].unlink(missing_ok=True)
                else:
                    fresh_files.append(cache_info)
                    total_size += cache_info["size"]

            # If still too large, remove oldest files until under limit
            if total_size > self.max_size_bytes:
                fresh_files.sort(key=lambda x: x["mtime"])  # Oldest first
                for cache_info in fresh_files:
                    if total_size <= self.max_size_bytes:
                        break
                    total_size -= cache_info["size"]
                    cache_info["path"].unlink(missing_ok=True)

        except OSError:
            # If we can't manage the cache, just continue - better to lose cache than break functionality
            pass

    def clear(self) -> int:
        """Clear all cache entries.

        Returns:
            Number of cache files removed
        """
        count = 0
        try:
            for cache_file in self.cache_dir.glob("*.json"):
                try:
                    cache_file.unlink()
                    count += 1
                except OSError:
                    pass
        except OSError:
            pass
        return count

    def stats(self) -> Dict[str, Any]:
        """Get cache statistics.

        Returns:
            Dictionary with cache statistics
        """
        try:
            cache_files = list(self.cache_dir.glob("*.json"))
            total_size = 0
            valid_count = 0
            current_time = time.time()

            for cache_file in cache_files:
                try:
                    stat = cache_file.stat()
                    age = current_time - stat.st_mtime
                    if age <= self.max_age_seconds:
                        valid_count += 1
                        total_size += stat.st_size
                except OSError:
                    # Remove unreadable files
                    cache_file.unlink(missing_ok=True)

            return {
                "cache_dir": str(self.cache_dir),
                "total_files": len(cache_files),
                "valid_files": valid_count,
                "total_size_bytes": total_size,
                "total_size_mb": round(total_size / (1024 * 1024), 2),
                "max_size_mb": round(self.max_size_bytes / (1024 * 1024), 2),
                "max_age_hours": round(self.max_age_seconds / 3600, 2),
            }
        except Exception as e:
            return {
                "error": str(e),
                "cache_dir": str(self.cache_dir)
            }


# Global inspection cache instance
_inspection_cache = None


def get_inspection_cache() -> InspectionCache:
    """Get or create the global inspection cache instance."""
    global _inspection_cache
    if _inspection_cache is None:
        _inspection_cache = InspectionCache()
    return _inspection_cache


def cached_inspection(prefix: str):
    """Decorator for inspection commands to add caching support.

    Args:
        prefix: Command-specific prefix for cache key generation

    Returns:
        Decorator function
    """
    def decorator(func):
        def wrapper(tc, args, *func_args, **func_kwargs):
            # Check if JSON output and caching are requested
            as_json = getattr(args, "json", False)
            no_cache = getattr(args, "no_cache", False)

            if as_json and not no_cache:
                # Try to get cached result
                cache_data = get_inspection_cache().get(
                    prefix,
                    paths=getattr(args, "paths", []),
                    # Add all relevant arguments that affect the output
                    **_extract_cache_args(args)
                )
                if cache_data is not None:
                    print(json.dumps(cache_data, indent=2))
                    return

            # Call the original function
            result = func(tc, args, *func_args, **func_kwargs)

            # Cache the result if JSON output was requested
            if as_json and not no_cache:
                # For non-JSON output functions, we need to capture the output
                # This decorator assumes the function handles its own output
                # For JSON output functions, they should handle their own caching
                pass

            return result
        return wrapper
    return decorator


def _extract_cache_args(args) -> Dict[str, Any]:
    """Extract arguments that affect command output for cache key generation.

    This is a simplified version - in practice, each command would need
    to specify which arguments affect its output.
    """
    # Common arguments that often affect output
    cache_args = {}
    for attr in dir(args):
        if not attr.startswith('_'):
            value = getattr(args, attr)
            # Only include basic types that are JSON serializable
            if isinstance(value, (str, int, float, bool, type(None))):
                cache_args[attr] = value
            elif isinstance(value, list) and all(isinstance(x, (str, int, float, bool, type(None))) for x in value):
                cache_args[attr] = value

    return cache_args