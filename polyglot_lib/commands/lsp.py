"""The 'lsp' command: launch the clangd language server with project
configuration derived from polyglot.toml.

clangd discovers a compile database by walking up from each opened file.
When the project's polyglot.toml carries a [flags] section (includes /
defines / std), polyglot writes a generated compile_flags.txt under
<root>/.polyglot-cache/lsp (so a hand-maintained one in the project root
is never clobbered) and points clangd at it with --compile-commands-dir.
The driver then replaces itself with clangd via os.execvp: editor and
server speak LSP over the inherited stdio, which is why the one-line
status note goes to stderr - it must not pollute the LSP channel.
"""

import os
import sys
from pathlib import Path

from polyglot_lib import toml
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_MISSING_TOOL, EXIT_USAGE, PolyglotError, quote,
)

ORDER = 145


def project_flags(config, base_dir):
    """clangd compile flags from a polyglot.toml [flags] table.

    Returns (flags, summary): the flag list (-I dirs absolutized against
    base_dir, -D defines, -std=<std>) plus a short human summary of what
    was configured.  A missing or empty [flags] table yields ([], "").
    Malformed values raise a friendly usage error (exit 2).
    """
    section = (config or {}).get("flags")
    if not section:
        return [], ""
    if not isinstance(section, dict):
        raise PolyglotError("polyglot.toml [flags] must be a table with "
                            "includes/defines/std keys", EXIT_USAGE)
    includes = section.get("includes") or []
    defines = section.get("defines") or []
    std = section.get("std")
    if not isinstance(includes, list) or not isinstance(defines, list):
        raise PolyglotError("polyglot.toml [flags] 'includes' and 'defines' "
                            "must be lists", EXIT_USAGE)
    if std is not None and not isinstance(std, str):
        raise PolyglotError("polyglot.toml [flags] 'std' must be a string",
                            EXIT_USAGE)
    flags = []
    for inc in includes:
        if not isinstance(inc, str):
            raise PolyglotError("polyglot.toml [flags] includes entries must "
                                "be strings (got %r)" % (inc,), EXIT_USAGE)
        p = Path(inc)
        if not p.is_absolute():
            p = Path(base_dir) / p
        flags.append("-I" + str(p.resolve()))
    for define in defines:
        if not isinstance(define, str):
            raise PolyglotError("polyglot.toml [flags] defines entries must "
                                "be strings (got %r)" % (define,), EXIT_USAGE)
        flags.append("-D" + define)
    if std:
        flags.append("-std=" + std)
    summary = ", ".join(part for part in (
        "%d include dir(s)" % len(includes) if includes else "",
        "%d define(s)" % len(defines) if defines else "",
        "std=%s" % std if std else "",
    ) if part)
    return flags, summary


def write_compile_flags(root, flags):
    """Write the generated compile_flags.txt for clangd.

    The file lives under <root>/.polyglot-cache/lsp/ so it never clobbers
    a hand-maintained compile_flags.txt in the project root and is swept
    away by 'polyglot clean'.  Returns the directory holding it (to hand
    to clangd via --compile-commands-dir), or None when it could not be
    written (the launch then degrades to plain clangd).
    """
    target_dir = Path(root) / ".polyglot-cache" / "lsp"
    try:
        target_dir.mkdir(parents=True, exist_ok=True)
        (target_dir / "compile_flags.txt").write_text(
            "\n".join(flags) + "\n")
    except OSError:
        return None
    return target_dir


@command("lsp", require=False)
def cmd_lsp(tc, args):
    roots = [Path(r) for r in (getattr(args, "roots", None) or [])]
    if not roots:
        roots = [Path.cwd()]
    for r in roots:
        if not r.is_dir():
            raise PolyglotError("root is not a directory: %s" % r, EXIT_USAGE)
    root = roots[0].resolve()

    clangd = tc.find_tool("clangd") if tc is not None else None
    if clangd is None:
        raise PolyglotError(
            "clangd not found (needed for the language server)\n"
            "Searched: PATH, the compiler's bin directory and LLVM install "
            "dirs\nHint: clangd ships with clang; install an LLVM/clang "
            "release (e.g. 'apt install clangd-17') or add its bin directory "
            "to PATH.", EXIT_MISSING_TOOL)

    config = None
    config_path = root / "polyglot.toml"
    if config_path.exists():
        if toml is None:
            print("lsp: toml module unavailable; ignoring %s" % config_path,
                  file=sys.stderr)
        else:
            try:
                config = toml.load(config_path)
            except Exception as exc:
                print("lsp: warning: could not parse %s: %s"
                      % (config_path, exc), file=sys.stderr)

    flags, summary = [], ""
    if config is not None:
        flags, summary = project_flags(config, root)

    exec_args = [str(clangd)]
    if flags:
        cdb_dir = write_compile_flags(root, flags)
        if cdb_dir is not None:
            exec_args.append("--compile-commands-dir=%s" % cdb_dir)
            note = ("lsp: configured clangd from polyglot.toml: %s "
                    "(compile_flags.txt: %s)"
                    % (summary, cdb_dir / "compile_flags.txt"))
        else:
            note = ("lsp: could not write compile_flags.txt under %s; "
                    "launching clangd without project flags" % root)
    elif config is not None:
        note = ("lsp: polyglot.toml has no [flags] includes/defines/std; "
                "launching clangd with defaults")
    else:
        note = ("lsp: no polyglot.toml in %s; launching clangd with defaults"
                % root)
    if len(roots) > 1:
        note += " (%d roots)" % len(roots)
    print(note, file=sys.stderr)
    if getattr(args, "verbose", False):
        print("lsp: exec %s" % quote(exec_args), file=sys.stderr)

    sys.stdout.flush()
    sys.stderr.flush()
    try:
        os.execvp(str(clangd), exec_args)
    except OSError as exc:
        raise PolyglotError("cannot launch clangd: %s" % exc)


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "lsp", help="Launch the clangd language server (project flags applied)",
        description="Launch clangd (the LSP server) for editor integration. "
                    "When polyglot.toml has a [flags] section, a generated "
                    "compile_flags.txt (includes/defines/std) is written "
                    "under <root>/.polyglot-cache/lsp and clangd is pointed "
                    "at it; the status note goes to stderr so the LSP stdio "
                    "channel stays clean.")
    p.add_argument("--roots", nargs="*", metavar="DIR",
                   help="workspace roots; the first root hosts polyglot.toml "
                        "and the generated compile_flags.txt (default: the "
                        "current directory)")
