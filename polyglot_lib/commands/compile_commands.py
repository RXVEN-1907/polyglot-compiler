"""The 'compile-commands' command: generate a compilation database for tooling.

Generates a compile_commands.json file that can be used by clangd, clang-tidy,
clang-format, and other tools that expect the standard compilation database
format. Each entry contains the full command used to compile a source file.
"""

import argparse
import json
import sys
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError
from polyglot_lib.toolchain import Toolchain
from polyglot_lib import VERSION

ORDER = 140  # Place before lsp (145) in help output


def resolve_compile_inputs(args):
    """Shared input resolution for compile-commands.

    Returns (sources, project_dir). Exits friendly on bad inputs.
    """
    paths = [Path(p) for p in (args.paths or ["."])]
    output = Path(args.output) if getattr(args, "output", None) else None

    # Try to find project directory first
    from polyglot_lib.config import find_project_dir
    project_dir, config = find_project_dir(paths)

    sources = []
    if project_dir is not None and config:
        # Use project-based resolution
        from polyglot_lib.config import (
            project_sources_and_output,
            parse_bin_entries,
            parse_lib_entries
        )

        # For compile-commands, we want all sources that would be compiled
        lang = config.get("language", {})
        if lang.get("rust"):
            # Rust project - look for main.rs
            src_dir = project_dir / "src"
            rust_src = src_dir / "main.rs"
            if rust_src.exists():
                sources = [rust_src]
            else:
                # Look for any .rs files in src/
                sources = list(src_dir.glob("*.rs")) if src_dir.exists() else []
        else:
            # C/C++ project
            sources, _ = project_sources_and_output(project_dir, config, output or Path("a.out"))

        # If no sources found via project config, fall back to globbing
        if not sources:
            sources = []
            for ext in [".c", ".cpp", ".cc", ".cxx", ".c++"]:
                sources.extend(sorted((project_dir / "src").glob("*" + ext)))
                sources.extend(sorted(project_dir.glob("*" + ext)))
    else:
        # No project directory, treat paths as direct file specifications
        if not paths:
            raise PolyglotError("nothing to compile", EXIT_USAGE)

        for f in paths:
            f = Path(f)
            if not f.exists():
                raise PolyglotError("Source file not found: %s" % f)
            if f.is_dir():
                # If it's a directory, look for source files in it
                for ext in [".c", ".cpp", ".cc", ".cxx", ".c++"]:
                    sources.extend(sorted(f.glob("*" + ext)))
            else:
                sources.append(f)

    # Deduplicate sources while preserving order
    seen = set()
    unique_sources = []
    for s in sources:
        if s not in seen:
            seen.add(s)
            unique_sources.append(s)

    if not unique_sources:
        raise PolyglotError("no source files found to compile")

    return unique_sources, project_dir


def _get_source_language_flags(tc, args, source_file):
    """Get compiler flags appropriate for the source file language."""
    # Use common_compile_args from backends for standard flags
    from polyglot_lib.backends import common_compile_args

    # Create a temporary args-like object with the source file
    class TempArgs:
        def __init__(self, args, source_file):
            self.std = getattr(args, 'std', None)
            self.warn = getattr(args, 'warn', 'default')
            self.werror = getattr(args, 'werror', False)
            self.debug = getattr(args, 'debug', False)
            self.sanitize = getattr(args, 'sanitize', None)
            self.includes = getattr(args, 'includes', [])
            self.defines = getattr(args, 'defines', [])
            self.cc_args = getattr(args, 'cc_args', [])
            self.opt_level = getattr(args, 'opt_level', 'default')
            self.target = getattr(args, 'target', None)
            self.shared = getattr(args, 'shared', False)
            self.static_lib = getattr(args, 'static_lib', False)
            self.output = getattr(args, 'output', None)

    temp_args = TempArgs(args, source_file)

    # Get common compile flags (includes defines, std, optimization, etc.)
    flags = common_compile_args(tc, temp_args, [str(source_file)])

    # For Rust, we need different handling
    if source_file.suffix == ".rs":
        # Clear the C/C++ flags and use Rust-specific ones
        flags = []
        std = getattr(args, 'std', None)
        if std:
            flags.append(f"-C{std}")  # Rough approximation - Rust uses -C flag format

        opt_level = getattr(args, 'opt_level', 'default')
        rust_opt_map = {
            "none": "-Copt-level=0",
            "less": "-Copt-level=1",
            "default": "-Copt-level=2",
            "aggressive": "-Copt-level=3",
            "size": "-Copt-level=z",
            "size-aggressive": "-Copt-level=z",
        }
        if opt_level in rust_opt_map:
            flags.append(rust_opt_map[opt_level])

        if getattr(args, 'debug', False):
            flags.append("-Cdebuginfo=2")

        # Rust doesn't use -I/-D in the same way, but we can approximate
        for inc in getattr(args, 'includes', []):
            flags.append(f"-L{inc}")  # Not quite right but best we can do without deeper Rust integration
        for define in getattr(args, 'defines', []):
            flags.append(f"-D{define}")  # Rust uses -D for defines too

        # Add raw cc args as-is for Rust (they might be cargo flags)
        flags.extend(getattr(args, 'cc_args', []))

        # Add target if specified
        target = getattr(args, 'target', None)
        if target and tc.kind == "clang":
            flags.append(f"--target={target}")

    return flags


@command("compile-commands")
def cmd_compile_commands(tc, args):
    """Generate a compilation database (compile_commands.json) for tooling.

    Creates a JSON file compatible with clangd, clang-tidy, clang-format,
    and other tools that expect the standard compilation database format.
    """
    try:
        sources, project_dir = resolve_compile_inputs(args)
    except PolyglotError as e:
        # Re-raise with proper exit code
        raise e

    # Determine output file
    output_file = Path(args.output) if getattr(args, "output", None) else Path("compile_commands.json")

    # Use project directory as the working directory for all entries
    # (this matches how compilation databases typically work)
    if project_dir is None:
        project_dir = Path.cwd()

    compilation_database = []

    for source_file in sources:
        if not source_file.exists():
            raise PolyglotError("Source file not found: %s" % source_file)

        # Get the appropriate compiler for this source file
        compiler = tc.compiler_for([str(source_file)])
        if compiler is None:
            raise PolyglotError("No suitable compiler found for %s" % source_file)

        # Get language-appropriate flags
        flags = _get_source_language_flags(tc, args, source_file)

        # Build the compile command
        cmd_parts = [str(compiler)] + flags

        # Add source file and output specification
        if source_file.suffix == ".rs":
            # Rust compilation: rustc src/main.rs -o binary
            output_name = getattr(args, "output", None)
            if output_name:
                exe_output = Path(output_name)
            else:
                exe_output = source_file.with_suffix("")  # Remove .rs extension

            cmd_parts.extend([str(source_file), "-o", str(exe_output)])
        else:
            # C/C++ compilation: compile to object file
            obj_output = source_file.with_suffix(".o")
            cmd_parts.extend(["-c", str(source_file), "-o", str(obj_output)])

        # Create the compilation database entry
        entry = {
            "directory": str(project_dir),
            "command": " ".join(cmd_parts),
            "file": str(source_file)
        }

        compilation_database.append(entry)

    # Write the compilation database as JSON
    try:
        with open(output_file, 'w') as f:
            json.dump(compilation_database, f, indent=2)
    except OSError as e:
        raise PolyglotError("Failed to write compilation database: %s" % e)

    print("Generated compilation database: %s (%d entries)" % (output_file, len(compilation_database)))


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "compile-commands",
        help="Generate compilation database for tooling (clangd, clang-tidy, etc.)",
        description="Create a compile_commands.json file that enables IDE integration, "
                    "static analysis, and other tooling. Each entry contains the "
                    "full command used to compile a source file."
    )
    p.add_argument("paths", nargs="*", help="source files or directories (default: current directory)")
    p.add_argument("-o", "--output", metavar="FILE", help="output file for compilation database (default: compile_commands.json)")
    p.add_argument("--target", help="target triple (clang cross or host on gcc)")
    p.add_argument("--shared", action="store_true", help="build a shared library (affects command generation)")
    p.add_argument("--static-lib", action="store_true", help="build a static library (ar rcs) (affects command generation)")
    # Add common flags that affect compilation
    global_flags.add_common_flags(p, link=False)