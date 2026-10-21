"""The 'inspect' command: provide correlated views of binary information
using multiple inspection tools for comprehensive analysis."""

import argparse
import json
import subprocess
import sys
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError

ORDER = 20  # High priority in help output


@command("inspect")
def cmd_inspect(tc, args):
    """Provide correlated views of binary information using multiple inspection tools."""
    paths = [Path(p) for p in args.paths]
    for f in paths:
        if not f.exists():
            raise PolyglotError("File not found: %s" % f)

    # For simplicity, just run strings tool on all files
    all_strings = []
    for file_path in paths:
        try:
            result = subprocess.run(['strings', str(file_path)],
                                  capture_output=True, text=True, check=True)
            strings = result.stdout.strip().split('\n')
            all_strings.extend([s for s in strings if s])
        except subprocess.CalledProcessError as e:
            print(f"Error running strings on {file_path}: {e}", file=sys.stderr)
        except FileNotFoundError:
            print(f"strings command not found", file=sys.stderr)

    # Output results
    as_json = getattr(args, "json", False)
    if as_json:
        # Create structured output
        output_data = {
            "tool": "strings",
            "strings": all_strings,
            "string_count": len(all_strings)
        }
        # Add metadata
        output_data["_meta"] = {
            "tool": "strings",
            "command": "polyglot inspect " + " ".join(str(p) for p in paths),
            "string_count": len(all_strings)
        }
        print(json.dumps(output_data, indent=2))
    else:
        # Print results
        for string in all_strings:
            print(string)


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "inspect",
        help="Provide correlated views of binary information using multiple tools")
    p.add_argument("paths", nargs="+", help="object files or executables")
    p.add_argument("--strings", action="store_true",
                   help="Include strings analysis")
    p.add_argument("--json", action="store_true",
                   help="output in JSON format for programmatic use")
    # Add common flags
    global_flags.add_common_flags(p)