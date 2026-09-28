"""The 'analyze' command: clang static analyzer (--analyze)."""

import argparse
import plistlib
import sys
import tempfile
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_MISSING_TOOL, PolyglotError, log_running, run_cmd,
)

ORDER = 150


@command("analyze")
def cmd_analyze(tc, args):
    if tc.kind != "clang":
        raise PolyglotError(
            "the static analyzer requires the clang backend "
            "(retry with: polyglot --compiler clang analyze ...)",
            EXIT_MISSING_TOOL)
    bad = []
    with tempfile.TemporaryDirectory() as tmpdir:
        for i, f in enumerate(args.paths):
            if not Path(f).exists():
                raise PolyglotError("Source file not found: %s" % f)
            # reports go to a scratch dir: --analyze exits 0 even when it
            # finds defects, and without -o a stray <name>.plist would be
            # dropped into the current directory
            report = Path(tmpdir) / ("report_%d.plist" % i)
            cmd = [str(tc.cc_path), "--analyze", "-o", str(report), str(f)]
            log_running(cmd, args)
            proc = run_cmd(cmd, capture_output=True, text=True)
            sys.stderr.write(proc.stderr or "")
            if proc.returncode != 0:
                bad.append(str(f))
                continue
            try:
                with open(report, "rb") as fh:
                    data = plistlib.load(fh)
            except (OSError, ValueError, plistlib.InvalidFileException):
                data = {}
            if data.get("diagnostics"):
                bad.append(str(f))
    if bad:
        raise PolyglotError("static analysis reported issues in: %s"
                            % ", ".join(bad), EXIT_FAILURE)


def register(subparsers, global_flags):
    p = subparsers.add_parser("analyze", help="clang static analyzer (--analyze)")
    p.add_argument("paths", nargs="+", help="source files")
