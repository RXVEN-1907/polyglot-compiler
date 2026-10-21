"""The 'objdump' command: display information from object files."""

import argparse
import json
from pathlib import Path
from typing import Dict, Any, List, Optional

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError
from polyglot_lib.inspection_cache import get_inspection_cache
from polyglot_lib.commands.inspection_utils import _parse_objdump_output

ORDER = 160


@command("objdump", pass_through=True)
def cmd_objdump(tc, args):
    """Display information using objdump or llvm-objdump with optional JSON output and caching."""
    # Handle JSON output flag
    as_json = getattr(args, "json", False)
    no_cache = getattr(args, "no_cache", False)

    # Check for cached JSON result
    if as_json and not no_cache:
        cache_data = get_inspection_cache().get(
            "objdump",
            paths=getattr(args, "paths", []),
            archive_headers=getattr(args, "archive_headers", False),
            archive_symbols=getattr(args, "archive_symbols", False),
            bfd=getattr(args, "bfd", None),
            disassemble=getattr(args, "disassemble", False),
            disassemble_all=getattr(args, "disassemble_all", False),
            file_headers=getattr(args, "file_headers", False),
            section_headers=getattr(args, "section_headers", False),
            section=getattr(args, "section", None),
            line_numbers=getattr(args, "line_numbers", False),
            architecture=getattr(args, "architecture", None),
            disassembler_options=getattr(args, "disassembler_options", None),
            private_headers=getattr(args, "private_headers", False),
            reloc=getattr(args, "reloc", False),
            dynamic_reloc=getattr(args, "dynamic_reloc", False),
            full_contents=getattr(args, "full_contents", False),
            source=getattr(args, "source", False),
            syms=getattr(args, "syms", False),
            dynamic_syms=getattr(args, "dynamic_syms", False),
            unwind_info=getattr(args, "unwind_info", False),
            no_demangle=getattr(args, "no_demangle", False),
            all_headers=getattr(args, "all_headers", False),
            disassemble_zeroes=getattr(args, "disassemble_zeroes", False),
        )
        if cache_data is not None:
            print(json.dumps(cache_data, indent=2))
            return

    # Select tool based on compiler backend to respect user's toolchain choice
    # When using clang, prefer llvm-objdump; when using gcc, prefer GNU objdump
    from polyglot_lib.toolchain import Toolchain
    if isinstance(tc, Toolchain) and tc.kind == "clang":
        # Prefer llvm-objdump for clang toolchain
        objdump_path = tc.find_tool("llvm-objdump")
        if objdump_path is None:
            objdump_path = tc.find_tool("objdump")
    else:
        # Prefer GNU objdump for gcc toolchain (or when kind is unknown/None)
        objdump_path = tc.find_tool("objdump")
        if objdump_path is None:
            objdump_path = tc.find_tool("llvm-objdump")

    if objdump_path is None:
        objdump_path = tc.tool_or_error("llvm-objdump" if isinstance(tc, Toolchain) and tc.kind == "clang" else "objdump", "displaying object file information")

    # Build objdump command
    cmd = [str(objdump_path)]

    # Add common flags
    if getattr(args, "archive_headers", False):
        cmd.append("-a")
    if getattr(args, "archive_symbols", False):
        cmd.append("--archive-symbols")
    if getattr(args, "bfd"):
        cmd.extend(["-b", getattr(args, "bfd")])
    if getattr(args, "disassemble", False):
        cmd.append("-d")
    if getattr(args, "disassemble_all", False):
        cmd.append("-D")
    if getattr(args, "file_headers", False):
        cmd.append("-f")
    if getattr(args, "section_headers", False):
        cmd.append("--section-headers")
    if getattr(args, "section"):
        cmd.extend(["-j", getattr(args, "section")])
    if getattr(args, "line_numbers", False):
        cmd.append("-l")
    if getattr(args, "architecture"):
        cmd.extend(["-m", getattr(args, "architecture")])
    if getattr(args, "disassembler_options"):
        cmd.extend(["-M", getattr(args, "disassembler_options")])
    if getattr(args, "private_headers", False):
        cmd.append("-p")
    if getattr(args, "reloc", False):
        cmd.append("-r")
    if getattr(args, "dynamic_reloc", False):
        cmd.append("-R")
    if getattr(args, "full_contents", False):
        cmd.append("-s")
    if getattr(args, "source", False):
        cmd.append("-S")
    if getattr(args, "syms", False):
        cmd.append("-t")
    if getattr(args, "dynamic_syms", False):
        cmd.append("-T")
    if getattr(args, "unwind_info", False):
        cmd.append("-u")
    if getattr(args, "no_demangle", False):
        cmd.append("-W")
    if getattr(args, "all_headers", False):
        cmd.append("-x")
    if getattr(args, "disassemble_zeroes", False):
        cmd.append("-z")

    # Add file paths
    cmd.extend([str(p) for p in (args.paths or [])])

    # Run the tool
    import subprocess
    import sys

    try:
        result = subprocess.run(cmd, capture_output=True, text=True, check=True)

        # Handle JSON output
        if as_json:
            # Parse the output into structured format
            lines = result.stdout.strip().split('\n')
            structured_data = _parse_objdump_output(lines)

            # Add metadata
            structured_data["_meta"] = {
                "tool": "llvm-objdump" if "llvm-objdump" in str(objdump_path) else "objdump",
                "command": " ".join(cmd),
                "return_code": result.returncode
            }

            # Cache the result
            if not no_cache:
                get_inspection_cache().set(
                    "objdump",
                    structured_data,
                    paths=getattr(args, "paths", []),
                    archive_headers=getattr(args, "archive_headers", False),
                    archive_symbols=getattr(args, "archive_symbols", False),
                    bfd=getattr(args, "bfd", None),
                    disassemble=getattr(args, "disassemble", False),
                    disassemble_all=getattr(args, "disassemble_all", False),
                    file_headers=getattr(args, "file_headers", False),
                    section_headers=getattr(args, "section_headers", False),
                    section=getattr(args, "section", None),
                    line_numbers=getattr(args, "line_numbers", False),
                    architecture=getattr(args, "architecture", None),
                    disassembler_options=getattr(args, "disassembler_options", None),
                    private_headers=getattr(args, "private_headers", False),
                    reloc=getattr(args, "reloc", False),
                    dynamic_reloc=getattr(args, "dynamic_reloc", False),
                    full_contents=getattr(args, "full_contents", False),
                    source=getattr(args, "source", False),
                    syms=getattr(args, "syms", False),
                    dynamic_syms=getattr(args, "dynamic_syms", False),
                    unwind_info=getattr(args, "unwind_info", False),
                    no_demangle=getattr(args, "no_demangle", False),
                    all_headers=getattr(args, "all_headers", False),
                    disassemble_zeroes=getattr(args, "disassemble_zeroes", False),
                )

            print(json.dumps(structured_data, indent=2))
        else:
            # Traditional output
            sys.stdout.write(result.stdout)
            if result.stderr:
                sys.stderr.write(result.stderr)
            sys.exit(result.returncode)

    except subprocess.CalledProcessError as e:
        # Handle JSON error output
        if as_json:
            error_data = {
                "error": f"objdump tool failed with return code {e.returncode}",
                "stdout": e.stdout,
                "stderr": e.stderr,
                "_meta": {
                    "tool": "llvm-objdump" if "llvm-objdump" in str(objdump_path) else "objdump",
                    "command": " ".join(cmd),
                    "return_code": e.returncode
                }
            }
            print(json.dumps(error_data, indent=2))
        else:
            sys.stderr.write(e.stdout or "")
            sys.stderr.write(e.stderr or "")
        sys.exit(e.returncode)
    except OSError as e:
        raise PolyglotError("failed to execute objdump tool: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("objdump", help="Display information from object files")
    p.add_argument("paths", nargs="*", help="object files or executables")
    # Common flags that objdump accepts (GNU binutils and llvm-objdump)
    p.add_argument("-a", "--archive-headers", action="store_true",
                   help="display archive headers")
    p.add_argument("--archive-symbols", action="store_true",
                   help="display archive symbols")
    p.add_argument("-b", "--bfd", metavar="BFDNAME",
                   help="specify BFD format")
    p.add_argument("-d", "--disassemble", action="store_true",
                   help="disassemble executable sections")
    p.add_argument("-D", "--disassemble-all", action="store_true",
                   help="disassemble all sections")
    p.add_argument("-f", "--file-headers", action="store_true",
                   help="display file headers")
    p.add_argument("--section-headers", "--headers", action="store_true",
                   help="display section headers")
    p.add_argument("-j", "--section", metavar="SECTIONNAME",
                   help="display information for section SECTIONNAME")
    p.add_argument("-l", "--line-numbers", action="store_true",
                   help="use line numbers in disassembly")
    p.add_argument("-m", "--architecture", metavar="MACHINE",
                   help="specify architecture")
    p.add_argument("-M", "--disassembler-options", metavar="OPTIONS",
                   help="pass target-specific options to disassembler")
    p.add_argument("-p", "--private-headers", action="store_true",
                   help="display object file specific headers")
    p.add_argument("-r", "--reloc", action="store_true",
                   help="display relocation entries")
    p.add_argument("-R", "--dynamic-reloc", action="store_true",
                   help="display dynamic relocation entries")
    p.add_argument("-s", "--full-contents", action="store_true",
                   help="display full contents of sections")
    p.add_argument("-S", "--source", action="store_true",
                   help="intermix source code with disassembly")
    p.add_argument("-t", "--syms", action="store_true",
                   help="display symbol table")
    p.add_argument("-T", "--dynamic-syms", action="store_true",
                   help="display dynamic symbol table")
    p.add_argument("-u", "--unwind-info", action="store_true",
                   help="display unwind info")
    p.add_argument("-W", "--no-demangle", action="store_true",
                   help="do not demangle C++ symbols (LLVM objdump only)")
    p.add_argument("-x", "--all-headers", action="store_true",
                   help="display all headers")
    p.add_argument("-z", "--disassemble-zeroes", action="store_true",
                   help="disassemble zero-filled sections")
    # Polyglot-specific enhancements
    p.add_argument("--json", action="store_true",
                   help="output in JSON format for programmatic use")
    p.add_argument("--no-cache", action="store_true",
                   help="disable caching of inspection results")