"""Argument parsing and dispatch for the polyglot driver.

build_parser() assembles the top-level parser and global flags; every
subcommand contributes its own subparser through its register() function
(auto-discovered in polyglot_lib.commands), so this module never needs
editing when commands are added.
"""

import argparse
import sys
from types import SimpleNamespace

from polyglot_lib import VERSION
from polyglot_lib import commands
from polyglot_lib.backends import OPT_MAP, SANITIZERS
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_USAGE, PolyglotError, err_print, quote,
)
from polyglot_lib.toolchain import Toolchain


def add_common_flags(p, link=False):
    p.add_argument("--std", metavar="STD",
                   help="language standard (c89..c2x / gnu*, c++98..c++23)")
    p.add_argument("--warn", choices=["none", "default", "all", "extra",
                                      "pedantic"], default="default",
                   help="warning level (default: default)")
    p.add_argument("--werror", action="store_true", help="warnings as errors")
    p.add_argument("--debug", action="store_true", help="emit debug info (-g)")
    p.add_argument("--sanitize", metavar="LIST",
                   help="sanitizers: %s" % ",".join(SANITIZERS))
    p.add_argument("-I", dest="includes", action="append", default=[],
                   metavar="DIR", help="include search path (repeatable)")
    p.add_argument("-D", dest="defines", action="append", default=[],
                   metavar="NAME[=VAL]", help="preprocessor define (repeatable)")
    p.add_argument("--cc-arg", dest="cc_args", action="append", default=[],
                   metavar="ARG", help="raw flag passed to the compiler (repeatable)")
    if link:
        p.add_argument("-L", dest="lib_dirs", action="append", default=[],
                       metavar="DIR", help="library search path (repeatable)")
        p.add_argument("-l", dest="libs", action="append", default=[],
                       metavar="NAME", help="link library (repeatable)")


# Shared flag helpers handed to every command's register().
GLOBAL_FLAGS = SimpleNamespace(
    add_common_flags=add_common_flags,
    OPT_MAP=OPT_MAP,
    SANITIZERS=SANITIZERS,
)


def build_parser():
    parser = argparse.ArgumentParser(
        prog="polyglot", description="Universal Compiler Driver (v%s)" % VERSION,
        epilog="run 'polyglot <command> --help' for per-command options")
    parser.add_argument("--version", action="version",
                        version="polyglot %s" % VERSION,
                        help="show the version and exit")
    parser.add_argument("-v", "--verbose", action="store_true",
                        help="verbose output (full command lines, cache HIT/MISS)")
    parser.add_argument("--quiet", action="store_true",
                        help="suppress 'Running:' lines")
    parser.add_argument("--target", help="target triple (clang cross or host on gcc)")
    parser.add_argument("--opt-level",
                        choices=list(OPT_MAP), default="default",
                        help="optimization level (default: default)")
    parser.add_argument("-o", "--output", help="output file")
    parser.add_argument("--plugin", help="JSON-RPC plugin binary (name or "
                        "path; used by 'build')")
    parser.add_argument("--plugin-dir", default="~/.polyglot/plugins",
                        help="plugin search directory (default: ~/.polyglot/plugins)")
    parser.add_argument("--compiler", choices=["auto", "clang", "gcc"],
                        default="auto",
                        help="force a backend (default: auto-discover; "
                             "env POLYGLOT_CC/POLYGLOT_CXX override)")

    sub = parser.add_subparsers(dest="command", required=True)

    for register in commands.REGISTRARS:
        register(sub, GLOBAL_FLAGS)

    # Global-style flags also work after the subcommand (SUPPRESS defaults
    # keep the global values intact; guards avoid redefining existing ones).
    def _ensure(sub_p, *opts, **kw):
        have = {o for a in sub_p._actions for o in getattr(a, "option_strings", [])}
        if not any(o in have for o in opts):
            sub_p.add_argument(*opts, **kw)

    for sub_p in sub.choices.values():
        _ensure(sub_p, "-v", "--verbose", action="store_true",
                default=argparse.SUPPRESS, help=argparse.SUPPRESS)
        _ensure(sub_p, "--quiet", action="store_true",
                default=argparse.SUPPRESS, help=argparse.SUPPRESS)
        _ensure(sub_p, "--compiler", choices=["auto", "clang", "gcc"],
                default=argparse.SUPPRESS, help=argparse.SUPPRESS)
        _ensure(sub_p, "--target", default=argparse.SUPPRESS,
                help=argparse.SUPPRESS)
        _ensure(sub_p, "--opt-level", choices=list(OPT_MAP),
                default=argparse.SUPPRESS, help=argparse.SUPPRESS)
        _ensure(sub_p, "--plugin", default=argparse.SUPPRESS,
                help=argparse.SUPPRESS)
        _ensure(sub_p, "--plugin-dir", default=argparse.SUPPRESS,
                help=argparse.SUPPRESS)

    return parser


# program-argument pass-through commands: tokens after the first '--' belong
# to the child program / tool, not to polyglot.
PASS_THROUGH_CMDS = {name for name, entry in commands.COMMANDS.items()
                     if entry["pass_through"]}


# Options that consume a separate value token. When one of these appears
# before the subcommand, its value must not be mistaken for the command name
# (e.g. the 'size' in '--opt-level size build ...' vs the 'size' command).
VALUE_FLAGS = {
    "--target", "--opt-level", "-o", "--output",
    "--plugin", "--plugin-dir", "--compiler",
    "--std", "--warn", "--sanitize",
    "-I", "-D", "--cc-arg", "-L", "-l",
}


def _find_subcommand(argv):
    """Index of the first token naming a known subcommand.

    Matching against the known command names (instead of heuristically
    skipping flags) keeps value-taking flags like `-I dir` or `--cc-arg x`
    from being mistaken for the subcommand when they appear before it.
    """
    i = 0
    while i < len(argv):
        tok = argv[i]
        if tok in commands.COMMANDS:
            return i
        # '--flag=value' carries its value in the same token; a bare flag
        # consumes the next token, so skip both.
        i += 2 if tok in VALUE_FLAGS else 1
    return None


def _split_at_ddash(tokens):
    if "--" in tokens:
        i = tokens.index("--")
        return tokens[:i], tokens[i + 1:]
    return tokens, None


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)

    extra_program_args = None
    idx = _find_subcommand(argv)
    if idx is not None and idx > 0:
        # common flags typed before the subcommand are re-ordered after it so
        # the subparser (which mirrors the global flags) actually sees them;
        # otherwise parse_known_args drops them silently.
        argv = [argv[idx]] + argv[:idx] + argv[idx + 1:]
        idx = 0
    if idx is not None and argv[idx] in PASS_THROUGH_CMDS:
        head, tail = _split_at_ddash(argv)
        argv = head
        extra_program_args = tail

    parser = build_parser()
    args, unknown = parser.parse_known_args(argv)

    if args.command in PASS_THROUGH_CMDS:
        if extra_program_args is not None:
            passthrough = extra_program_args
        else:
            passthrough = [u for u in unknown if u != "--"]
            unknown = []
        if args.command == "run":
            args.args = passthrough
        else:
            args.extra = passthrough

    if unknown:
        err_print("error: unrecognized arguments: %s" % quote(unknown))
        sys.exit(EXIT_USAGE)

    try:
        entry = commands.COMMANDS[args.command]
        if entry["toolchain"]:
            tc = Toolchain(prefer=args.compiler,
                           defer=entry["defer"],
                           require=entry["require"])
            entry["handler"](tc, args)
        else:
            entry["handler"](None, args)
    except PolyglotError as exc:
        err_print("error: %s" % exc)
        sys.exit(exc.exit_code)
    except KeyboardInterrupt:
        print("\nInterrupted.")
        sys.exit(130)
    except SystemExit:
        raise
    except Exception as exc:  # never show a traceback to end users
        err_print("unexpected error: %r" % exc)
        if getattr(args, "verbose", False):
            import traceback
            traceback.print_exc(file=sys.stderr)
        sys.exit(EXIT_FAILURE)


if __name__ == "__main__":
    main()
