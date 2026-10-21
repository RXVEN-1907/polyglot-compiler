"""
Polyglot - Universal Compiler Driver (v0.2.0)

A driver that discovers and drives clang / GCC toolchains: build, run,
check, IR/ASM emission, formatting, linting, JIT, static analysis,
incremental caching and more.  Backend-aware: the same user interface
works over clang or GCC, translating flags as needed.

Requires only the Python 3 standard library plus the 'toml' package.
The implementation lives in this package; the executable 'polyglot'
launcher next to it is a thin wrapper around polyglot_lib.cli.main.
"""

from pathlib import Path

try:
    import toml
except ImportError:  # pragma: no cover - toml is only needed for project files
    toml = None

VERSION = "0.2.0"

# Directory containing this package - i.e. the directory holding the
# executable 'polyglot' launcher.  Plugin resolution scans
# <this dir>/target/release, matching the old single-file driver, which
# resolved against the launcher script's own directory.
SCRIPT_DIR = Path(__file__).resolve().parent.parent