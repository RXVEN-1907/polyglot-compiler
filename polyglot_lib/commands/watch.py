"""The 'watch' command: rebuild on file changes, optionally re-running
the built program after each successful rebuild (--test)."""

import argparse
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

from polyglot_lib.backends import common_compile_args, compile_to_exe
from polyglot_lib.commands import command
from polyglot_lib.output import (
    PolyglotError, exit_status, green, red, run_cmd, split_program_args,
)

ORDER = 160

# --test output: how many program output lines to show per run, and how
# long one run may take before it is abandoned (the watch itself keeps
# going; a hung test must not hang the loop).
TEST_OUTPUT_LINES = 20
TEST_TIMEOUT = 60


@command("watch", pass_through=True)
def cmd_watch(tc, args):
    src = Path(args.file)
    if not src.exists():
        raise PolyglotError("Source file not found: %s" % src)
    tmpdir = tempfile.mkdtemp(prefix="polyglot_watch_")
    exe = Path(tmpdir) / "watch_out"
    quiet = args.quiet
    args.quiet = True          # watch output stays readable
    prog_args = split_program_args(getattr(args, "extra", None) or [])
    deadline = time.time() + args.timeout if args.timeout else None
    last_mtime = None

    def stamp():
        return time.strftime("%H:%M:%S")

    def run_test():
        """Run the freshly built program; report exit code and the first
        output lines.  A failing run prints red but never stops the watch."""
        if not exe.exists():
            return
        try:
            proc = run_cmd([str(exe)] + prog_args, capture_output=True,
                           text=True, stdin=subprocess.DEVNULL,
                           timeout=TEST_TIMEOUT)
        except subprocess.TimeoutExpired:
            print(red("[%s] test timed out after %ds"
                      % (stamp(), TEST_TIMEOUT)))
            return
        except OSError as exc:
            print(red("[%s] test could not run: %s" % (stamp(), exc)))
            return
        out = (proc.stdout or "").splitlines()
        err = (proc.stderr or "").splitlines()
        for line in out[:TEST_OUTPUT_LINES]:
            print("    " + line)
        if len(out) > TEST_OUTPUT_LINES:
            print("    ... (%d more stdout line(s) truncated)"
                  % (len(out) - TEST_OUTPUT_LINES))
        for line in err[:TEST_OUTPUT_LINES]:
            print("    " + line)
        if len(err) > TEST_OUTPUT_LINES:
            print("    ... (%d more stderr line(s) truncated)"
                  % (len(err) - TEST_OUTPUT_LINES))
        if proc.returncode == 0:
            print(green("[%s] test ok (exit 0)" % stamp()))
        else:
            print(red("[%s] test failed (exit %d)"
                      % (stamp(), exit_status(proc.returncode))))

    def rebuild():
        t0 = time.time()
        try:
            extra = common_compile_args(tc, args, [src])
            compile_to_exe(tc, [src], exe, args, extra)
        except PolyglotError as exc:
            print(red("[%s] rebuild failed: %s" % (stamp(), exc)))
            return
        ms = int((time.time() - t0) * 1000)
        print(green("[%s] rebuilt ok (%dms)" % (stamp(), ms)))
        if args.test:
            run_test()

    try:
        rebuild()
        last_mtime = src.stat().st_mtime
        while True:
            time.sleep(0.5)
            try:
                mtime = src.stat().st_mtime
            except OSError:
                continue
            if mtime != last_mtime:
                last_mtime = mtime
                rebuild()
            if deadline and time.time() >= deadline:
                break
    finally:
        args.quiet = quiet
        shutil.rmtree(tmpdir, ignore_errors=True)
    # Ctrl-C propagates to main()'s handler ("Interrupted.", exit 130)


def register(subparsers, global_flags):
    p = subparsers.add_parser("watch", help="Rebuild on file changes")
    p.add_argument("file", help="source file to watch")
    p.add_argument("--test", action="store_true",
                   help="after each successful rebuild, run the binary and "
                        "report its exit code and first output lines; a "
                        "failing run prints red but the watch continues "
                        "(program args go after '--')")
    p.add_argument("--timeout", type=int, default=0,
                   help="exit after N seconds (0 = watch forever)")
    global_flags.add_common_flags(p)
    p.set_defaults(_pass_through=True)   # program args arrive via extras
