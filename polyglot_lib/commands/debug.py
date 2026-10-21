"""The 'debug' command: build a source file with debug info and launch a debugger.

Compiles the source with -g flags and launches lldb (for clang) or gdb (for GCC)
on the resulting executable, passing any tokens after '--' to the debugger.
"""

import argparse
import sys
import tempfile
from pathlib import Path
from types import SimpleNamespace

from polyglot_lib.backends import common_compile_args, compile_to_exe
from polyglot_lib.commands import command
from polyglot_lib.output import (
    PolyglotError, exit_status, run_cmd,
)
from polyglot_lib.toolchain import CXX_SUFFIXES

ORDER = 170  # Place after preview (155) and before less-used commands


@command("debug", pass_through=True)
def cmd_debug(tc, args):
    """Build with debug info and launch lldb/gdb."""
    sources = [Path(args.file)]
    if not sources[0].exists():
        raise PolyglotError("Source file not found: %s" % sources[0])

    # Determine if we're compiling C or C++ based on file extension
    is_cxx = sources[0].suffix in CXX_SUFFIXES

    with tempfile.TemporaryDirectory() as tmpdir:
        exe_path = Path(tmpdir) / "polyglot_debug"

        # Ensure debug info is always included for debugging
        # Create a copy of args with debug forced to True
        debug_args = SimpleNamespace(**vars(args))
        debug_args.debug = True

        # Get common compile args (including -g due to forced debug=True)
        extra = common_compile_args(tc, debug_args, sources)

        # Compile the executable
        compile_to_exe(tc, sources, exe_path, debug_args, extra)

        # Determine which debugger to use based on the compiler kind
        debugger_name = "lldb" if tc.kind == "clang" else "gdb"

        try:
            debugger_path = tc.tool_or_error(debugger_name,
                                           "launching debugger (%s)" % debugger_name)
        except PolyglotError:
            # Fallback: try the other debugger if the preferred one isn't found
            fallback_name = "gdb" if tc.kind == "clang" else "lldb"
            try:
                debugger_path = tc.tool_or_error(fallback_name,
                                               "launching debugger (%s)" % fallback_name)
                debugger_name = fallback_name
            except PolyglotError:
                raise PolyglotError(
                    "neither lldb nor gdb found (needed for debugging)\n"
                    "Hint: install lldb (comes with clang) or gdb "
                    "(comes with GCC) and ensure they are in PATH."
                )

        # Launch the debugger on the executable
        # Pass any user-provided arguments after '--' to the debugger
        debugger_args = [str(debugger_path), str(exe_path)]
        if hasattr(args, 'extra') and args.extra:
            debugger_args.extend(args.extra)

        # Execute the debugger (replace current process)
        import os
        os.execv(str(debugger_path), debugger_args)


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "debug",
        help="Build with debug info and launch debugger (lldb/gdb)",
        description="Compile source with -g flags and launch lldb (for clang) "
                    "or gdb (for GCC) on the resulting executable. "
                    "Any arguments after '--' are passed to the debugger."
    )
    p.add_argument("file", help="source file to debug")
    # Add common flags that affect compilation
    global_flags.add_common_flags(p, link=True)
    p.set_defaults(_pass_through=True)   # debugger args arrive via extras