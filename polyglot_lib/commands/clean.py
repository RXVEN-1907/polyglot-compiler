"""The 'clean' command: remove build artifacts and the incremental cache.

Runs without any toolchain discovery (tc is None): cleaning must work even
when no compiler is installed.
"""

import argparse
from pathlib import Path

from polyglot_lib import toml
from polyglot_lib.cache import BuildCache
from polyglot_lib.commands import command
from polyglot_lib.output import err_print

ORDER = 190


@command("clean", toolchain=False)
def cmd_clean(tc, args):
    project_dir = Path(args.path)
    removed = 0
    patterns = ["*.o", "*.obj", "*.exe", "a.out", "*.ll", "*.s", "*.bc"]
    for pattern in patterns:
        for f in project_dir.glob(pattern):
            if f.is_file():
                try:
                    f.unlink()
                    print("Removed %s" % f)
                    removed += 1
                except OSError as exc:
                    err_print("could not remove %s: %s" % (f, exc))

    config_file = project_dir / "polyglot.toml"
    if config_file.exists() and toml is not None:
        try:
            config = toml.load(config_file)
            for bin_cfg in config.get("bin", []):
                name = bin_cfg.get("name")
                if name:
                    continue  # executables are left alone by default
        except Exception:
            pass

    cache = BuildCache(project_dir)
    removed += cache.clear()
    for f in project_dir.glob(".polyglot-cache/*.tmp"):
        try:
            f.unlink()
            removed += 1
        except OSError:
            pass

    if removed == 0:
        print("No build artifacts to clean")
    else:
        print("Cleaned %d artifacts" % removed)


def register(subparsers, global_flags):
    p = subparsers.add_parser("clean", help="Remove build artifacts and cache")
    p.add_argument("path", nargs="?", default=".", help="project directory")
