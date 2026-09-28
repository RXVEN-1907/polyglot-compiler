"""The 'doctor' command: report on the toolchain environment."""

import json
import os
import shutil
import sys
from pathlib import Path

from polyglot_lib import SCRIPT_DIR, VERSION
from polyglot_lib.commands import command
from polyglot_lib.output import EXIT_FAILURE, PolyglotError, green, red, yellow
from polyglot_lib.toolchain import AUX_TOOLS, CLANG_ONLY_NAMES, Toolchain

ORDER = 80


def detect_plugins(plugin_dir):
    """Executable plugin candidates for the doctor report: everything in
    --plugin-dir plus polyglot-* binaries in target/release (deduped by
    name, plugin-dir first)."""
    found = {}
    roots = [Path(plugin_dir).expanduser()]
    script_dir = SCRIPT_DIR
    for base in (script_dir / "target" / "release",
                 Path.cwd() / "target" / "release"):
        if base not in roots:
            roots.append(base)
    for root in roots:
        release_scan = root.name == "release" and root.parent.name == "target"
        try:
            entries = sorted(root.iterdir())
        except OSError:
            continue
        for f in entries:
            if not (f.is_file() and os.access(f, os.X_OK)):
                continue
            if release_scan and not (f.name.startswith("polyglot-")
                                     and f.name != "polyglot-driver"):
                continue
            found.setdefault(f.name, f)
    return [found[k] for k in sorted(found)]


def _problem_names(tc):
    """Labels of the missing components, in report order (same labels the
    human report prints as MISS)."""
    names = []
    if tc.cc_path is None:
        names.append("C compiler")
    if tc.cxx_path is None:
        names.append("C++ compiler")
    for name in AUX_TOOLS:
        if tc.find_tool(name) is None:
            names.append(name)
    return names


def _json_report(tc, args):
    """Machine-readable doctor payload: {driver, compilers, backend, tools,
    plugins, cache, hints}."""
    cache_dir = Path.cwd() / ".polyglot-cache"
    cache_exists = cache_dir.is_dir()
    objs = list(cache_dir.glob("*.o")) if cache_exists else []
    size = sum(f.stat().st_size for f in objs if f.is_file())

    hints = []
    if tc.cc_path is None:
        hints.append("no C compiler found (install clang/gcc or set "
                     "POLYGLOT_CC)")
    elif tc.found_off_path:
        hints.append("clang was found off-PATH; consider: export PATH=%s:$PATH"
                     % tc.cc_path.parent)
    if tc.cc_path is not None and shutil.which("clang") is None:
        clang_off = Toolchain._find_by_names(CLANG_ONLY_NAMES)[0]
        if clang_off is not None:
            hints.append("clang is not on PATH but exists at %s; consider: "
                         "export PATH=%s:$PATH" % (clang_off, clang_off.parent))

    tools = {}
    for name in AUX_TOOLS:
        p = tc.find_tool(name)
        tools[name] = {"ok": p is not None, "path": str(p) if p else None}

    plugins = detect_plugins(getattr(args, "plugin_dir", None)
                             or "~/.polyglot/plugins")
    return {
        "driver": {"name": "polyglot", "version": VERSION,
                   "python": ".".join(map(str, sys.version_info[:3]))},
        "compilers": {
            "c": {"ok": tc.cc_path is not None,
                  "path": str(tc.cc_path) if tc.cc_path else None,
                  "version": tc.version if tc.cc_path else None},
            "cxx": {"ok": tc.cxx_path is not None,
                    "path": str(tc.cxx_path) if tc.cxx_path else None},
        },
        "backend": tc.kind,
        "tools": tools,
        "plugins": [str(p) for p in plugins],
        "cache": {"dir": str(cache_dir), "exists": cache_exists,
                  "objects": len(objs), "size_bytes": size},
        "hints": hints,
    }


def _human_report(tc, args):
    """The classic doctor output; returns the list of missing components."""
    problems = []

    def line(label, ok, detail):
        mark = green("ok  ") if ok else red("MISS")
        print("  %s %-14s %s" % (mark, label, detail))
        if not ok:
            problems.append(label)

    print("=" * 62)
    print("POLYGLOT ENVIRONMENT REPORT (doctor)")
    print("=" * 62)
    print("  driver          polyglot %s (python %s)"
          % (VERSION, ".".join(map(str, sys.version_info[:3]))))

    if tc.cc_path is None:
        line("C compiler", False,
             "not found (install clang/gcc or set POLYGLOT_CC)")
    else:
        line("C compiler", True, "%s [%s]" % (tc.cc_path, tc.version))
    if tc.found_off_path:
        print(yellow("  hint            clang was found off-PATH; consider:\n"
                     "                    export PATH=%s:$PATH"
                     % tc.cc_path.parent))
    if tc.cxx_path:
        line("C++ compiler", True, str(tc.cxx_path))
    else:
        line("C++ compiler", False, "clang++/g++ not found (C++ builds disabled)")
    print("  backend         %s" % (tc.kind or "none"))

    plugins = detect_plugins(getattr(args, "plugin_dir", None)
                             or "~/.polyglot/plugins")
    print("  %-18s%s" % ("plugins",
                        ", ".join(str(p) for p in plugins)
                        if plugins else "none detected "
                        "(see 'Plugin architecture' in docs/user-guide/commands.md)"))

    if shutil.which("clang") is None:
        clang_off = Toolchain._find_by_names(CLANG_ONLY_NAMES)[0]
        if clang_off is not None:
            print(yellow("  hint            clang is not on PATH but exists at:\n"
                         "                    %s\n"
                         "                  consider: export PATH=%s:$PATH"
                         % (clang_off, clang_off.parent)))

    print("-" * 62)
    print("Auxiliary tools:")
    for name, purpose in AUX_TOOLS.items():
        p = tc.find_tool(name)
        line(name, p is not None, str(p) if p else "(%s)" % purpose)

    print("-" * 62)
    cache_dir = Path.cwd() / ".polyglot-cache"
    if cache_dir.is_dir():
        objs = list(cache_dir.glob("*.o"))
        size = sum(f.stat().st_size for f in objs if f.is_file())
        print("  build cache     %s (%d object(s), %.1f KiB)"
              % (cache_dir, len(objs), size / 1024.0))
    else:
        print("  build cache     %s (empty - will be created on first build)"
              % cache_dir)

    print("=" * 62)
    return problems


@command("doctor", require=False)
def cmd_doctor(tc, args):
    as_json = getattr(args, "json", False)
    if as_json:
        problems = _problem_names(tc)
        print(json.dumps(_json_report(tc, args), indent=2))
    else:
        problems = _human_report(tc, args)

    if problems and getattr(args, "strict", False):
        raise PolyglotError("doctor --strict: %d component(s) missing: %s"
                            % (len(problems), ", ".join(problems)), EXIT_FAILURE)
    if not as_json:
        if problems:
            print(yellow("%d component(s) missing; affected features degrade "
                         "gracefully." % len(problems)))
        else:
            print(green("All components present."))


def register(subparsers, global_flags):
    p = subparsers.add_parser("doctor", help="Report on the toolchain environment")
    p.add_argument("--strict", action="store_true",
                   help="exit 1 when anything is missing")
    p.add_argument("--json", action="store_true",
                   help="machine-readable JSON output")
