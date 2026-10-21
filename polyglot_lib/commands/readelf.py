"""The 'readelf' command: display the contents of ELF files."""

import argparse
import json
from pathlib import Path
from typing import Dict, Any, List, Optional

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError
from polyglot_lib.inspection_cache import get_inspection_cache
from polyglot_lib.commands.inspection_utils import _parse_readelf_output

ORDER = 165


@command("readelf", pass_through=True)
def cmd_readelf(tc, args):
    """Display ELF file contents using readelf or llvm-readelf with optional JSON output and caching."""
    # Handle JSON output flag
    as_json = getattr(args, "json", False)
    no_cache = getattr(args, "no_cache", False)

    # Check for cached JSON result
    if as_json and not no_cache:
        cache_data = get_inspection_cache().get(
            "readelf",
            paths=getattr(args, "paths", []),
            all=getattr(args, "all", False),
            arch_specific=getattr(args, "arch_specific", False),
            dynamic=getattr(args, "dynamic", False),
            file_header=getattr(args, "file_header", False),
            program_header=getattr(args, "program_header", False),
            section_header=getattr(args, "section_header", False),
            symbols=getattr(args, "symbols", False),
            relocs=getattr(args, "relocs", False),
            dynamic_relocs=getattr(args, "dynamic_relocs", False),
            version_info=getattr(args, "version_info", False),
            program_headers=getattr(args, "program_headers", False),
            histogram=getattr(args, "histogram", False),
            notes=getattr(args, "notes", False),
            phdr=getattr(args, "phdr", False),
            section_details=getattr(args, "section_details", False),
            unwind=getattr(args, "unwind", False),
            version=getattr(args, "version", False),
            verbose=getattr(args, "verbose", False),
            hex_dump=getattr(args, "hex", None),
            hash_table=getattr(args, "hash", False),
        )
        if cache_data is not None:
            print(json.dumps(cache_data, indent=2))
            return

    # Select tool based on compiler backend to respect user's toolchain choice
    # When using clang, prefer llvm-readelf; when using gcc, prefer GNU readelf
    from polyglot_lib.toolchain import Toolchain
    if isinstance(tc, Toolchain) and tc.kind == "clang":
        # Prefer llvm-readelf for clang toolchain
        readelf_path = tc.find_tool("llvm-readelf")
        if readelf_path is None:
            readelf_path = tc.find_tool("readelf")
    else:
        # Prefer GNU readelf for gcc toolchain (or when kind is unknown/None)
        readelf_path = tc.find_tool("readelf")
        if readelf_path is None:
            readelf_path = tc.find_tool("llvm-readelf")

    if readelf_path is None:
        readelf_path = tc.tool_or_error("llvm-readelf" if isinstance(tc, Toolchain) and tc.kind == "clang" else "readelf", "displaying ELF file contents")

    # Build readelf command
    cmd = [str(readelf_path)]

    # Add common flags
    if getattr(args, "all", False):
        cmd.append("-a")
    if getattr(args, "arch_specific", False):
        cmd.append("--arch-specific")
    if getattr(args, "dynamic", False):
        cmd.append("-d")
    if getattr(args, "file_header", False):
        cmd.append("--file-header")
    if getattr(args, "program_header", False):
        cmd.append("-l")
    if getattr(args, "section_header", False):
        cmd.append("-S")
    if getattr(args, "symbols", False):
        cmd.append("-s")
    if getattr(args, "relocs", False):
        cmd.append("-r")
    if getattr(args, "dynamic_relocs", False):
        cmd.append("-R")
    if getattr(args, "version_info", False):
        cmd.append("-V")
    if getattr(args, "program_headers", False):
        cmd.append("-i")
    if getattr(args, "histogram", False):
        cmd.append("-I")
    if getattr(args, "notes", False):
        cmd.append("-n")
    if getattr(args, "phdr", False):
        cmd.append("-p")
    if getattr(args, "section_details", False):
        cmd.append("-t")
    if getattr(args, "unwind", False):
        cmd.append("-u")
    if getattr(args, "version", False):
        cmd.append("-v")
    if getattr(args, "verbose", False):
        cmd.append("-w")
    if getattr(args, "hex", None):
        cmd.extend(["-x", getattr(args, "hex")])
    if getattr(args, "hash", False):
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
            structured_data = _parse_readelf_output(lines)

            # Add metadata
            structured_data["_meta"] = {
                "tool": "llvm-readelf" if "llvm-readelf" in str(readelf_path) else "readelf",
                "command": " ".join(cmd),
                "return_code": result.returncode
            }

            # Cache the result
            if not no_cache:
                get_inspection_cache().set(
                    "readelf",
                    structured_data,
                    paths=getattr(args, "paths", []),
                    all=getattr(args, "all", False),
                    arch_specific=getattr(args, "arch_specific", False),
                    dynamic=getattr(args, "dynamic", False),
                    file_header=getattr(args, "file_header", False),
                    program_header=getattr(args, "program_header", False),
                    section_header=getattr(args, "section_header", False),
                    symbols=getattr(args, "symbols", False),
                    relocs=getattr(args, "relocs", False),
                    dynamic_relocs=getattr(args, "dynamic_relocs", False),
                    version_info=getattr(args, "version_info", False),
                    program_headers=getattr(args, "program_headers", False),
                    histogram=getattr(args, "histogram", False),
                    notes=getattr(args, "notes", False),
                    phdr=getattr(args, "phdr", False),
                    section_details=getattr(args, "section_details", False),
                    unwind=getattr(args, "unwind", False),
                    version=getattr(args, "version", False),
                    verbose=getattr(args, "verbose", False),
                    hex_dump=getattr(args, "hex", None),
                    hash_table=getattr(args, "hash", False),
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
                "error": f"readelf tool failed with return code {e.returncode}",
                "stdout": e.stdout,
                "stderr": e.stderr,
                "_meta": {
                    "tool": "llvm-readelf" if "llvm-readelf" in str(readelf_path) else "readelf",
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
        raise PolyglotError("failed to execute readelf tool: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("readelf", help="Display the contents of ELF files")
    p.add_argument("paths", nargs="*", help="ELF files or executables")
    # Common flags that readelf accepts (GNU binutils and llvm-readelf)
    p.add_argument("-a", "--all", action="store_true",
                   help="equivalent to: -h -l -S -s -r -d -V -A -I")
    p.add_argument("--arch-specific", action="store_true",
                   help="display architecture specific information")
    p.add_argument("-d", "--dynamic", action="store_true",
                   help="display dynamic section")
    p.add_argument("--file-header", action="store_true",
                   help="display ELF file header")
    p.add_argument("-l", "--program-header", "--segments", action="store_true",
                   help="display program headers")
    p.add_argument("-S", "--section-header", "--sections", action="store_true",
                   help="display section headers")
    p.add_argument("-s", "--symbols", "--syms", action="store_true",
                   help="display symbol table")
    p.add_argument("-r", "--relocs", action="store_true",
                   help="display relocation entries")
    p.add_argument("-R", "--dynamic-relocs", action="store_true",
                   help="display dynamic relocation entries")
    p.add_argument("-V", "--version-info", action="store_true",
                   help="display version sections")
    p.add_argument("-i", "--program-headers", action="store_true",
                   help="display program headers (aka segments)")
    p.add_argument("-I", "--histogram", action="store_true",
                   help="display histogram of bucket lengths")
    p.add_argument("-n", "--notes", action="store_true",
                   help="display notes sections")
    p.add_argument("-p", "--phdr", action="store_true",
                   help="display program headers")
    p.add_argument("-t", "--section-details", action="store_true",
                   help="display section details")
    p.add_argument("-u", "--unwind-info", action="store_true",
                   help="display unwind section")
    p.add_argument("-v", "--version", action="store_true",
                   help="display version sections")
    p.add_argument("-w", "--verbose", action="store_true",
                   help="display verbose information")
    p.add_argument("-x", "--hex", metavar="SECTIONNAME",
                   help="display hex dump of section")
    p.add_argument("-z", "--compress-debug-sections", action="store_true",
                   help="compress debug sections (LLVM readelf only)")
    p.add_argument("-H", "--hash", action="store_true",
                   help="display ELF hash table (LLVM readelf only)")
    # Polyglot-specific enhancements
    p.add_argument("--json", action="store_true",
                   help="output in JSON format for programmatic use")
    p.add_argument("--no-cache", action="store_true",
                   help="disable caching of inspection results")