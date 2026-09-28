"""Shared small helpers: colored output, friendly errors, exit codes,
command quoting and subprocess conveniences.  Imported by every other
module; depends on nothing else in polyglot_lib."""

import os
import shlex
import subprocess
import sys

EXIT_OK = 0
EXIT_FAILURE = 1
EXIT_USAGE = 2
EXIT_MISSING_TOOL = 3


# --------------------------------------------------------------------------
# Small helpers: colored output, errors, shell quoting
# --------------------------------------------------------------------------

def _color_enabled():
    return sys.stdout.isatty() and not os.environ.get("NO_COLOR")


def _paint(text, code, stream=sys.stdout):
    if getattr(stream, "isatty", lambda: False)() and not os.environ.get("NO_COLOR"):
        return "\033[%dm%s\033[0m" % (int(code), text)
    return str(text)


def green(text, stream=sys.stdout):
    return _paint(text, "32", stream)


def red(text, stream=sys.stdout):
    return _paint(text, "31", stream)


def yellow(text, stream=sys.stdout):
    return _paint(text, "33", stream)


def err_print(msg):
    print(red(msg, sys.stderr), file=sys.stderr)


class PolyglotError(Exception):
    """A friendly, user-facing error.  .exit_code decides the process exit."""

    def __init__(self, message, exit_code=EXIT_FAILURE):
        super().__init__(message)
        self.exit_code = exit_code


class MissingToolError(PolyglotError):
    """A backing tool could not be located."""

    def __init__(self, tool, searched=None, purpose=None):
        # main()'s handler already prefixes "error: " when printing.
        msg = "required tool '%s' not found" % tool
        if purpose:
            msg += " (needed for %s)" % purpose
        if searched:
            msg += "\nSearched: %s" % searched
        msg += "\nInstall it and re-run, or adjust PATH/POLYGLOT_CC."
        super().__init__(msg, EXIT_MISSING_TOOL)
        self.tool = tool


def quote(cmd):
    """Pretty-print a command list for logs (never used for execution)."""
    return " ".join(shlex.quote(str(c)) for c in cmd)


def run_cmd(cmd, **kwargs):
    """subprocess.run with list args only (never shell=True)."""
    return subprocess.run([str(c) for c in cmd], **kwargs)


def exit_status(rc):
    """Map a signal death (-N) to the conventional 128+N exit status."""
    return 128 - rc if rc < 0 else rc


def log_running(cmd, args):
    if getattr(args, "quiet", False):
        return
    print("Running: %s" % quote(cmd))


def split_program_args(raw_args):
    """Drop a single leading '--' separator, if present."""
    if raw_args and raw_args[0] == "--":
        return raw_args[1:]
    return list(raw_args)
