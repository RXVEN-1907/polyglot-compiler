"""The 'readelf' command: display the contents of ELF files."""

import argparse
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError

ORDER = 165


@command("readelf", pass_through=True)
def cmd_readelf(tc, args):
    """Display ELF file contents using llvm-readelf."""
    readelf_path = tc.tool_or_error("llvm-readelf", "displaying ELF file contents")
    # Forward all arguments to llvm-readelf
    import subprocess
    import sys

    cmd = [str(readelf_path)] + list(args.paths or [])
    try:
        result = subprocess.run(cmd, check=True)
        sys.exit(result.returncode)
    except subprocess.CalledProcessError as e:
        sys.exit(e.returncode)
    except OSError as e:
        raise PolyglotError("failed to execute llvm-readelf: %s" % e)


def register(subparsers, global_flags):
    p = subparsers.add_parser("readelf", help="Display the contents of ELF files")
    p.add_argument("paths", nargs="*", help="ELF files or executables")
    # Common flags that llvm-readelf accepts
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
    p.add_argument("-u", "--unwind", action="store_true",
                   help="display unwind section")
    p.add_argument("-v", "--version", action="store_true",
                   help="display version sections")
    p.add_argument("-w", "--verbose", action="store_true",
                   help="display verbose information")
    p.add_argument("-x", "--hex", metavar="SECTIONNAME",
                   help="display hex dump of section")
    p.add_argument("-z", "--compress-debug-sections", action="store_true",
                   help="compress debug sections")
    p.add_argument("-H", "--hash", action="store_true",
                   help="display ELF hash table")