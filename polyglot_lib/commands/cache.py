"""The 'cache' command: inspect and manage the incremental build cache.

Runs without any toolchain discovery (tc is None): cache inspection and
maintenance must work even when no compiler is installed.
"""

import argparse
import json
import re
import time
from pathlib import Path

from polyglot_lib.cache import BuildCache
from polyglot_lib.commands import command
from polyglot_lib.config import find_project_dir
from polyglot_lib.output import EXIT_USAGE, PolyglotError, yellow

ORDER = 175


# ---------------------------------------------------------------------------
# CLI-layer helpers: project resolution, size/duration parsing, formatting
# ---------------------------------------------------------------------------

def _project_dir(path_str):
    """Directory whose .polyglot-cache/ this command operates on.

    Honors polyglot.toml the same way build does, so 'cache stats' inside a
    project reports the cache that 'build' actually populates.
    """
    base = Path(path_str or ".")
    if not base.exists():
        raise PolyglotError("path not found: %s" % base, EXIT_USAGE)
    if not base.is_dir():
        raise PolyglotError("expected a directory, got file: %s" % base,
                            EXIT_USAGE)
    project, _config = find_project_dir([str(base)])
    return project if project is not None else base


_SIZE_UNITS = {
    "": 1, "b": 1,
    "k": 1024, "kb": 1024, "kib": 1024,
    "m": 1024 ** 2, "mb": 1024 ** 2, "mib": 1024 ** 2,
    "g": 1024 ** 3, "gb": 1024 ** 3, "gib": 1024 ** 3,
    "t": 1024 ** 4, "tb": 1024 ** 4, "tib": 1024 ** 4,
}
_DURATION_UNITS = {"s": 1, "m": 60, "h": 3600, "d": 86400, "w": 604800}


def parse_size(text):
    """'100MB' / '512KiB' / '1G' / '1048576' -> bytes (binary units)."""
    m = re.fullmatch(r"\s*(\d+(?:\.\d+)?)\s*([A-Za-z]*)\s*", str(text))
    if m is None:
        raise PolyglotError("invalid size: %s (try e.g. 100MB, 512KiB, 1G)"
                            % text, EXIT_USAGE)
    unit = m.group(2).lower()
    if unit not in _SIZE_UNITS:
        raise PolyglotError("unknown size unit '%s' in %s (use B, KB/KiB, "
                            "MB/MiB, GB/GiB, TB/TiB)" % (m.group(2), text),
                            EXIT_USAGE)
    return int(float(m.group(1)) * _SIZE_UNITS[unit])


def parse_duration(text):
    """'30d' / '12h' / '45m' / '90s' / '2w' / '30' -> seconds (bare = days)."""
    m = re.fullmatch(r"\s*(\d+(?:\.\d+)?)\s*([smhdwSMHDW]?)\s*", str(text))
    if m is None:
        raise PolyglotError("invalid duration: %s (try e.g. 30d, 12h, 45m)"
                            % text, EXIT_USAGE)
    unit = m.group(2).lower() or "d"
    return float(m.group(1)) * _DURATION_UNITS[unit]


def human_bytes(n):
    """1234 -> '1.2 KiB' style, for human tables."""
    size = float(n)
    for unit in ("B", "KiB", "MiB", "GiB", "TiB"):
        if size < 1024.0 or unit == "TiB":
            if unit == "B":
                return "%d B" % int(size)
            return "%.1f %s" % (size, unit)
        size /= 1024.0


def _clock(epoch):
    return time.strftime("%Y-%m-%d %H:%M", time.localtime(epoch))


def _short_source(source, width=48):
    text = str(source or "(unknown)")
    if len(text) <= width:
        return text
    return "..." + text[-(width - 3):]


def _rate_label(hit_rate):
    return "%.1f%%" % (hit_rate * 100.0) if hit_rate is not None \
        else "n/a (no lookups yet)"


# ---------------------------------------------------------------------------
# Actions
# ---------------------------------------------------------------------------

def _dir_label(cache):
    return str(Path(cache.dir).resolve())


def _do_stats(cache, args):
    s = cache.stats()
    s["dir"] = _dir_label(cache)
    if getattr(args, "json", False):
        print(json.dumps(s, indent=2))
        return
    print("=" * 62)
    print("BUILD CACHE")
    print("=" * 62)
    print("  directory       %s" % s["dir"])
    print("  entries         %d" % s["entries"])
    print("  total size      %s (%s bytes)"
          % (human_bytes(s["total_bytes"]),
             format(s["total_bytes"], ",")))
    print("  oldest entry    %s" % (s["oldest"] or "(none)"))
    print("  newest entry    %s" % (s["newest"] or "(none)"))
    print("  cache hits      %d" % s["hits"])
    print("  cache misses    %d" % s["misses"])
    print("  hit rate        %s" % _rate_label(s["hit_rate"]))
    if s["orphan_objects"]:
        print(yellow("  note            %d orphan object(s), %s not tracked "
                     "by the manifest ('cache verify' removes them)"
                     % (s["orphan_objects"], human_bytes(s["orphan_bytes"]))))
    print("=" * 62)


def _do_list(cache, args):
    entries = cache.entries()
    sort = args.sort
    if sort == "size":
        entries.sort(key=lambda e: e["size"], reverse=True)
    elif sort == "source":
        entries.sort(key=lambda e: (str(e["source"] or ""), e["key"]))
    else:                                   # age: newest first
        entries.sort(key=lambda e: e["mtime"], reverse=True)
    total = len(entries)
    limit = args.limit
    if limit is not None:
        if limit < 0:
            raise PolyglotError("--limit must be >= 0", EXIT_USAGE)
        shown = entries[:limit]
    else:
        shown = entries

    if getattr(args, "json", False):
        payload = {
            "dir": _dir_label(cache),
            "total": total,
            "shown": len(shown),
            "entries": [{
                "key": e["key"],
                "source": e["source"],
                "flags": e["flags"],
                "created": e["created"],
                "size_bytes": e["size"],
                "modified": _clock(e["mtime"]),
            } for e in shown],
        }
        print(json.dumps(payload, indent=2))
        return

    if not shown:
        print("(build cache is empty: %s)" % _dir_label(cache))
        return
    print("%-10s %-17s %-48s %s" % ("SIZE", "MODIFIED", "SOURCE", "KEY"))
    for e in shown:
        print("%-10s %-17s %-48s %s"
              % (human_bytes(e["size"]), _clock(e["mtime"]),
                 _short_source(e["source"]), e["key"][:12]))
    print("-" * 88)
    if limit is not None and total > len(shown):
        print("%d of %d object(s), %s total"
              % (len(shown), total,
                 human_bytes(sum(e["size"] for e in entries))))
    else:
        print("%d object(s), %s total"
              % (total, human_bytes(sum(e["size"] for e in entries))))


def _do_prune(cache, args):
    older = parse_duration(args.older_than) if args.older_than else None
    max_bytes = parse_size(args.max_bytes) if args.max_bytes else None
    if older is None and max_bytes is None:
        raise PolyglotError("nothing to prune: give --older-than and/or "
                            "--max-bytes", EXIT_USAGE)
    result = cache.prune(older_than=older, max_bytes=max_bytes)
    if result["removed"]:
        print("Pruned %d object(s), freed %s"
              % (result["removed"], human_bytes(result["bytes_freed"])))
    else:
        print("Nothing to prune (%d object(s) within limits)"
              % len(cache.entries()))


def _do_clear(cache, args):
    removed = cache.clear()
    if removed:
        print("Cleared %d file(s) from %s" % (removed, _dir_label(cache)))
    else:
        print("Build cache is already empty (%s)" % _dir_label(cache))


def _do_path(cache, args):
    print(Path(cache.dir).resolve())


def _do_verify(cache, args):
    tally = cache.verify()
    if getattr(args, "json", False):
        print(json.dumps(dict(dir=_dir_label(cache), **tally), indent=2))
        return
    print("Verified cache at %s" % _dir_label(cache))
    print("  objects checked       %d" % tally["checked"])
    print("  corrupt (removed)     %d" % tally["corrupt"])
    print("  missing (removed)     %d" % tally["missing"])
    print("  orphans (removed)     %d" % tally["orphan"])
    print("  legacy (left as-is)   %d" % tally["legacy"])
    print("  bytes freed           %s" % human_bytes(tally["bytes_freed"]))
    print("  remaining entries     %d" % tally["remaining"])


_ACTIONS = {
    "stats": _do_stats,
    "list": _do_list,
    "prune": _do_prune,
    "clear": _do_clear,
    "path": _do_path,
    "verify": _do_verify,
}


@command("cache", toolchain=False)
def cmd_cache(tc, args):
    action = getattr(args, "cache_cmd", None) or "stats"
    cache = BuildCache(_project_dir(getattr(args, "path", None) or "."))
    _ACTIONS[action](cache, args)


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "cache", help="Inspect and manage the build cache",
        description="Inspect and manage the per-project incremental build "
                    "cache (.polyglot-cache/).  With no action given, "
                    "'stats' is shown.")
    sub = p.add_subparsers(dest="cache_cmd")

    def nest(name, help_text):
        sp = sub.add_parser(name, help=help_text)
        sp.add_argument("path", nargs="?", default=".",
                        help="project directory (default: .)")
        sp.add_argument("-v", "--verbose", action="store_true",
                        default=argparse.SUPPRESS, help=argparse.SUPPRESS)
        sp.add_argument("--quiet", action="store_true",
                        default=argparse.SUPPRESS, help=argparse.SUPPRESS)
        return sp

    sp = nest("stats", "Summary: entries, total size, age, hit rate")
    sp.add_argument("--json", action="store_true",
                    help="machine-readable JSON output")

    sp = nest("list", "List cached objects")
    sp.add_argument("--sort", choices=["size", "age", "source"], default="age",
                    help="sort order (default: age, newest first)")
    sp.add_argument("--limit", type=int, metavar="N",
                    help="show only the first N entries")
    sp.add_argument("--json", action="store_true",
                    help="machine-readable JSON output")

    sp = nest("prune", "Remove stale entries (LRU by last use)")
    sp.add_argument("--older-than", metavar="AGE",
                    help="remove entries older than AGE, e.g. 30d, 12h, "
                         "45m, 2w (bare number = days)")
    sp.add_argument("--max-bytes", metavar="SIZE",
                    help="evict oldest-first until the cache fits SIZE, "
                         "e.g. 100MB, 1GiB")

    nest("clear", "Delete every cache file (like 'clean' does for the cache)")
    nest("path", "Print the cache directory")

    sp = nest("verify", "Recompute object checksums; drop corrupt, missing "
                        "and untracked files")
    sp.add_argument("--json", action="store_true",
                    help="machine-readable JSON output")
