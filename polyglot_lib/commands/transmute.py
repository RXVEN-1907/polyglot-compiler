"""The 'transmute' command: show the step-by-step compilation process with explanations."""

import argparse
import os
import tempfile
import textwrap
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError

ORDER = 170  # After objdump (160)


def _find_native_driver():
    """Find the Rust polyglot-driver binary."""
    import shutil
    found = shutil.which("polyglot-driver")
    if found:
        return Path(found)
    # Look in workspace target/release
    script_dir = Path(__file__).parent.parent.parent
    for base in (script_dir / "target" / "release",
                 Path.cwd() / "target" / "release"):
        cand = base / "polyglot-driver"
        try:
            if cand.is_file() and os.access(cand, os.X_OK):
                return cand
        except OSError:
            continue
    return None


@command("transmute")
def cmd_transmute(tc, args):
    """Show the step-by-step compilation process from source to IR."""
    src = Path(args.path)
    if not src.exists():
        raise PolyglotError("Source file not found: %s" % src)

    # Use the native Rust driver to get intermediate representations
    driver = _find_native_driver()
    if driver is None:
        raise PolyglotError(
            "polyglot-driver not found; build it with: cargo build --release "
            "-p polyglot-driver"
        )

    # Prepare arguments for the driver
    cmd = [str(driver), "transmute-internal", str(src)]

    # Add common compile arguments
    if getattr(args, "opt_level", None):
        cmd += ["--opt-level", str(args.opt_level)]
    if getattr(args, "target", None):
        cmd += ["--target", str(args.target)]
    for inc in getattr(args, "includes", None) or []:
        cmd += ["-I", str(inc)]
    for defin in getattr(args, "defines", None) or []:
        cmd += ["-D", str(defin)]
    if getattr(args, "warn", None):
        cmd += ["--warn", str(args.warn)]
    if getattr(args, "werror", False):
        cmd += ["--werror"]
    if getattr(args, "debug", False):
        cmd += ["--debug"]
    if getattr(args, "sanitize", None):
        cmd += ["--sanitize", str(args.sanitize)]
    for cc_arg in getattr(args, "cc_args", None) or []:
        cmd += ["--cc-arg", str(cc_arg)]

    # Show header
    print("; polyglot transmute: %s" % src)
    print("; showing the step-by-step compilation process")
    print("")

    # Run the internal command that shows all stages
    import subprocess
    import sys
    try:
        result = subprocess.run(cmd, check=True, capture_output=True, text=True)
        sys.stdout.write(result.stdout)
        if result.stderr:
            sys.stderr.write(result.stderr)
        sys.exit(result.returncode)
    except subprocess.CalledProcessError as e:
        sys.stdout.write(e.stdout)
        if e.stderr:
            sys.stderr.write(e.stderr)
        sys.exit(e.returncode)
    except OSError as e:
        raise PolyglotError("failed to execute polyglot-driver: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "transmute",
        help="Show the step-by-step compilation process with explanations")
    p.add_argument("path", metavar="FILE", help="source file to transmute")
    p.add_argument("--opt-level",
                   choices=list(global_flags.OPT_MAP), default="default",
                   help="optimization level (default: default)")
    p.add_argument("--target", help="target triple")
    global_flags.add_common_flags(p)