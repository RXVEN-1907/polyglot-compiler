"""The 'diagnose' command: compare diagnostics from multiple compilers.

Runs the source code through available compilers (clang, gcc) and presents
warnings/errors in a unified side-by-side format to highlight portability
issues and compiler-specific behaviors.
"""

import argparse
import itertools
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError
from polyglot_lib.toolchain import Toolchain

ORDER = 140  # Place before debug (170)


def _get_available_compilers():
    """Detect available compilers for comparison."""
    compilers = []

    # Get the toolchain instance
    tc = Toolchain()

    # Check for GCC
    if tc.cc_path and tc.kind == "gcc":
        compilers.append(("gcc", str(tc.cc_path)))
    elif tc.cc_path:  # It's clang, but we still want to check for gcc separately
        # Try to find gcc specifically
        gcc_path = tc.find_tool("gcc")
        if gcc_path:
            compilers.append(("gcc", str(gcc_path)))

    # Check for Clang
    clang_path = tc.find_tool("clang")
    if clang_path:
        compilers.append(("clang", str(clang_path)))
    else:
        # Try versioned clang names
        for version in range(21, 14, -1):  # clang-21 down to clang-15
            versioned_clang = tc.find_tool(f"clang-{version}")
            if versioned_clang:
                compilers.append(("clang", str(versioned_clang)))
                break

    # Also check polyglot's known clang location from doctor output
    import os
    home = os.path.expanduser("~")
    polyglot_clang = f"{home}/llvm-install/clang+llvm-17.0.6-x86_64-linux-gnu-ubuntu-22.04/bin/clang"
    if os.path.exists(polyglot_clang) and os.access(polyglot_clang, os.X_OK):
        # Avoid duplicates
        if not any(path == polyglot_clang for _, path in compilers):
            compilers.append(("clang", polyglot_clang))

    # Remove duplicates while preserving order (based on path)
    seen = set()
    unique_compilers = []
    for name, path in compilers:
        if path not in seen:
            seen.add(path)
            unique_compilers.append((name, path))

    return unique_compilers


def _parse_diagnostics(output, compiler_name):
    """Parse compiler output into structured diagnostics."""
    diagnostics = []
    lines = output.split('\n')

    i = 0
    while i < len(lines):
        line = lines[i]

        # Skip empty lines
        if not line.strip():
            i += 1
            continue

        # Match GCC/Clang diagnostic format:
        #   file:line:col: level: message (clang)
        #   file:line: level: message (gcc)
        # Be more specific to avoid matching summary lines like "1 warning generated."
        match = re.match(r'^(.+?):(\d+)(?::(\d+))?:\s+(warning|error|note):\s+(.+)$', line)
        if match:
            file_path, line_num, col, level, message = match.groups()
            # Additional validation: avoid false positives on summary lines
            # Summary lines don't look like file paths
            if '/' in file_path or '.' in file_path.split('/')[-1]:  # Looks like a file path
                line_num = int(line_num)
                col = int(col) if col is not None else 0  # Default column to 0 if not provided

                # Get the source line if available
                source_line = ""
                if i + 1 < len(lines) and lines[i + 1].strip() and not lines[i + 1].startswith(' '):
                    source_line = lines[i + 1].rstrip()
                    i += 1  # Skip the source line

                # Get any additional context lines (like ^~~~~~ pointers)
                context_lines = []
                j = i + 1
                while j < len(lines) and (lines[j].startswith(' ') or lines[j].startswith('^')):
                    context_lines.append(lines[j].rstrip())
                    j += 1
                i = j - 1  # Adjust for outer loop increment

                diagnostics.append({
                    'file': file_path,
                    'line': line_num,
                    'column': col,
                    'level': level,
                    'message': message.strip(),
                    'source_line': source_line,
                    'context': context_lines,
                    'compiler': compiler_name
                })

        i += 1

    return diagnostics


def _run_compiler(compiler_path, source_file, extra_args=None):
    """Run a compiler and capture its diagnostics output."""
    if extra_args is None:
        extra_args = []

    # Common args for diagnostic comparison
    args = [
        compiler_path,
        "-std=c17",
        "-Wall",
        "-Wextra",
        "-Wpedantic",
        "-fsyntax-only",
        str(source_file)
    ] + extra_args

    try:
        result = subprocess.run(
            args,
            capture_output=True,
            text=True,
            timeout=10
        )

        # Combine stdout and stderr as both can contain diagnostics
        output = result.stdout + result.stderr
        return result.returncode, output
    except subprocess.TimeoutExpired:
        return -1, f"Timeout running {compiler_path}"
    except Exception as e:
        return -1, f"Error running {compiler_path}: {e}"


def _format_diagnostic_side_by_side(all_diagnostics, source_lines=None):
    """Format diagnostics in a side-by-side view."""
    if not all_diagnostics:
        return "No diagnostics found."

    # Group diagnostics by file and line
    by_location = defaultdict(list)
    for diag in all_diagnostics:
        key = (diag['file'], diag['line'])
        by_location[key].append(diag)

    # Get unique compilers
    compilers = sorted(set(diag['compiler'] for diag in all_diagnostics))

    output_lines = []
    output_lines.append("Compiler Diagnostics Comparison")
    output_lines.append("=" * 50)
    output_lines.append("")

    # Show each file with its diagnostics
    for (file_path, line_num), diags in sorted(by_location.items()):
        output_lines.append(f"{file_path}:{line_num}:")
        output_lines.append("-" * 40)

        # Show source line if available
        source_line = ""
        for diag in diags:
            if diag['source_line']:
                source_line = diag['source_line']
                break

        if source_line:
            output_lines.append(f"    {source_line}")

            # Show pointer to column if we have column info
            # Find the minimum column among diagnostics at this location
            min_col = min((diag['column'] for diag in diags if diag['column'] > 0), default=0)
            if min_col > 0:
                pointer = " " * (min_col - 1) + "^"
                output_lines.append(f"    {pointer}")

        output_lines.append("")

        # Show diagnostics from each compiler
        for compiler in compilers:
            compiler_diags = [d for d in diags if d['compiler'] == compiler]
            if compiler_diags:
                output_lines.append(f"  {compiler}:")
                for diag in compiler_diags:
                    level_prefix = {
                        'warning': 'warning:',
                        'error': 'error:',
                        'note': 'note:'
                    }.get(diag['level'], diag['level'] + ':')

                    output_lines.append(f"    {level_prefix} {diag['message']}")

                    # Show context lines if available
                    for ctx_line in diag['context']:
                        output_lines.append(f"    {ctx_line}")
            else:
                output_lines.append(f"  {compiler}: (no diagnostics)")

        output_lines.append("")

    # Summary
    output_lines.append("Summary:")
    output_lines.append("-" * 40)
    for compiler in compilers:
        count = len([d for d in all_diagnostics if d['compiler'] == compiler])
        output_lines.append(f"  {compiler}: {count} diagnostic(s)")

    # Show agreements and disagreements
    agree_disagree = _analyze_agreement(all_diagnostics)
    if agree_disagree['agree']:
        output_lines.append("")
        output_lines.append("Agreed diagnostics (same message):")
        for msg in agree_disagree['agree']:
            output_lines.append(f"  - {msg}")

    if agree_disagree['disagree']:
        output_lines.append("")
        output_lines.append("Compiler-disagreed diagnostics:")
        for msg, details in agree_disagree['disagree'].items():
            output_lines.append(f"  - '{msg}':")
            for compiler, detail in details.items():
                output_lines.append(f"    {compiler}: {detail}")

    return "\n".join(output_lines)


def _analyze_agreement(all_diagnostics):
    """Analyze which diagnostics compilers agree/disagree on."""
    # Group by message similarity (simple string comparison for now)
    by_message = defaultdict(list)
    for diag in all_diagnostics:
        by_message[diag['message']].append(diag)

    agree = []  # Same message from multiple compilers
    disagree = {}  # Different messages or same message with different levels

    for message, diags in by_message.items():
        compilers = set(diag['compiler'] for diag in diags)
        levels = set(diag['level'] for diag in diags)

        if len(compilers) > 1 and len(levels) == 1:
            # Same message, same level, multiple compilers -> agreement
            agree.append(message)
        elif len(compilers) > 1:
            # Same message, different levels, or different messages -> disagreement
            disagree[message] = {
                diag['compiler']: f"{diag['level']}: {diag['message']}"
                for diag in diags
            }

    return {
        'agree': agree,
        'disagree': disagree
    }


@command("diagnose")
def cmd_diagnose(tc, args):
    """Compare diagnostics from multiple compilers."""
    if not args.paths:
        raise PolyglotError("diagnose: no input files", 2)

    # Validate that files exist
    for path_str in args.paths:
        if not Path(path_str).exists():
            raise PolyglotError(f"diagnose: no such file: {path_str}", 1)

    # Get available compilers
    compilers = _get_available_compilers()
    if len(compilers) < 2:
        raise PolyglotError(
            "diagnose: need at least 2 compilers for comparison "
            "(found {}). Install both clang and gcc.".format(len(compilers)),
            1
        )

    all_diagnostics = []
    failed_compilers = []

    # Run each compiler on each file
    for compiler_name, compiler_path in compilers:
        for path_str in args.paths:
            source_file = Path(path_str)
            returncode, output = _run_compiler(compiler_path, source_file)

            if returncode != 0 and not output.strip():
                failed_compilers.append((compiler_name, compiler_path, f"return code {returncode}"))
                continue

            if output.strip():
                diagnostics = _parse_diagnostics(output, compiler_name)
                all_diagnostics.extend(diagnostics)

    if failed_compilers:
        error_msg = "Failed to run compilers:\n"
        for name, path, reason in failed_compilers:
            error_msg += f"  {name} ({path}): {reason}\n"
        raise PolyglotError(error_msg.strip())

    if not all_diagnostics:
        print("No diagnostics found in any compiler.")
        return

    # Format and display results
    output = _format_diagnostic_side_by_side(all_diagnostics)
    print(output)


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "diagnose",
        help="Compare diagnostics from multiple compilers",
        description="Run source code through available compilers (clang, gcc) and show warnings/errors side-by-side"
    )
    p.add_argument("paths", nargs="+", help="source files to check")
    p.add_argument(
        "--std",
        help="language standard (c89..c2x / gnu*, c++98..c++23)",
        metavar="STD"
    )
    p.add_argument(
        "--warn",
        choices=["none", "default", "all", "extra", "pedantic"],
        help="warning level (default: default)"
    )
    p.add_argument(
        "--werror",
        action="store_true",
        help="treat warnings as errors"
    )