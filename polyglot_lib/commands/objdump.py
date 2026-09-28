"""The 'objdump' command: display information from object files."""

import argparse
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError

ORDER = 160


@command("objdump", pass_through=True)
def cmd_objdump(tc, args):
    """Display information using llvm-objdump."""
    objdump_path = tc.tool_or_error("llvm-objdump", "displaying object file information")
    # Forward all arguments to llvm-objdump
    import subprocess
    import sys

    cmd = [str(objdump_path)] + list(args.paths or [])
    try:
        result = subprocess.run(cmd, check=True)
        sys.exit(result.returncode)
    except subprocess.CalledProcessError as e:
        sys.exit(e.returncode)
    except OSError as e:
        raise PolyglotError("failed to execute llvm-objdump: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("objdump", help="Display information from object files")
    p.add_argument("paths", nargs="*", help="object files or executables")
    # Common flags that llvm-objdump accepts
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
                   help="do not demangle C++ symbols")
    p.add_argument("-x", "--all-headers", action="store_true",
                   help="display all headers")
    p.add_argument("-z", "--disassemble-zeroes", action="store_true",
                   help="disassemble zero-filled sections")