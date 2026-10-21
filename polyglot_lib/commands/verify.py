"""The 'verify' command: check that the toolchain can produce a valid object."""

import shutil
import subprocess
import sys
from pathlib import Path

from polyglot_lib.commands import command
from polyglot_lib.output import PolyglotError, err_print

ORDER = 70  # place near doctor/build


@command("verify", toolchain=True, require=True)
def cmd_verify(tc, args):
    """Verify the toolchain by compiling a tiny C program."""
    # Create a temporary C source file
    import tempfile

    with tempfile.TemporaryDirectory(prefix="polyglot-verify-") as tmpdir:
        src = Path(tmpdir) / "verify.c"
        src.write_text("int main(void) { return 0; }\n")
        obj = Path(tmpdir) / "verify.o"

        # Determine compiler command
        cc = tc.cc_path if tc.cc_path is not None else shutil.which("cc") or shutil.which("gcc") or shutil.which("clang")
        if cc is None:
            raise PolyglotError("No C compiler found (checked tc.cc_path, cc, gcc, clang)")

        # Compile to object file
        cmd = [str(cc), "-c", str(src), "-o", str(obj)]
        try:
            subprocess.run(cmd, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        except subprocess.CalledProcessError as e:
            err_print("error: verification compile failed:")
            err_print(e.stderr.decode() if isinstance(e.stderr, bytes) else e.stderr)
            sys.exit(1)
        except OSError as e:
            raise PolyglotError(f"failed to execute compiler: {e}")

        # Ensure object file was created and is non‑empty
        if not obj.is_file() or obj.stat().st_size == 0:
            raise PolyglotError("compilation succeeded but object file missing or empty")

        print("Toolchain verification successful: produced", obj)


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "verify", help="Verify the toolchain compiles a tiny program to an object")
    global_flags.add_common_flags(p)
