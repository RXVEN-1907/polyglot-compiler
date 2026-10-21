"""The 'cache' command: manage inspection cache statistics and operations."""

import json
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError
from polyglot_lib.inspection_cache import get_inspection_cache


ORDER = 180


@command("cache")
def cmd_cache(tc, args):
    """Manage inspection cache statistics and operations."""
    # Handle subcommands
    if hasattr(args, 'subcommand'):
        if args.subcommand == 'stats':
            stats = get_inspection_cache().stats()
            print(json.dumps(stats, indent=2))
        elif args.subcommand == 'clear':
            count = get_inspection_cache().clear()
            print(f"Cleared {count} cache entries")
        elif args.subcommand == 'info':
            cache = get_inspection_cache()
            info = {
                "cache_dir": str(cache.cache_dir),
                "max_size_mb": round(cache.max_size_bytes / (1024 * 1024), 2),
                "max_age_hours": round(cache.max_age_seconds / 3600, 2),
            }
            print(json.dumps(info, indent=2))
        else:
            raise PolyglotError(f"Unknown cache subcommand: {args.subcommand}")
    else:
        # Default behavior: show stats
        stats = get_inspection_cache().stats()
        print(json.dumps(stats, indent=2))


def register(subparsers, global_flags):
    p = subparsers.add_parser("cache", help="Manage inspection cache")
    subparsers = p.add_subparsers(dest='subcommand')

    # stats subcommand
    stats_p = subparsers.add_parser('stats', help='Show cache statistics')

    # clear subcommand
    clear_p = subparsers.add_parser('clear', help='Clear all cache entries')

    # info subcommand
    info_p = subparsers.add_parser('info', help='Show cache configuration')