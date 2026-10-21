"""The 'build' command plus the shared input resolution used by build-like
commands (sources/output/project resolution, rust project builds), and the
multi-target project build ([[bin]] x N / [[lib]] x N)."""

import argparse
import copy
import sys
from pathlib import Path

from polyglot_lib.backends import (
    RUST_OPT_MAP, common_compile_args, compile_to_exe, invoke_compiler,
)
from polyglot_lib.commands import command
from polyglot_lib.config import (
    find_project_dir, parse_bin_entries, parse_lib_entries,
    project_sources_and_output, resolve_bin_sources,
)
from polyglot_lib.output import (
    EXIT_USAGE, MissingToolError, PolyglotError, err_print,
)
from polyglot_lib.plugin_client import PluginError, build_with_plugin, choose_plugin

ORDER = 10

_SHARED_SUFFIXES = (".so", ".dylib", ".dll")


def resolve_build_inputs(args):
    """Shared input resolution for build-like commands.

    Returns (sources, output, project_dir).  Exits friendly on bad inputs.
    """
    paths = [Path(p) for p in (args.paths or ["."])]
    output = Path(args.output) if getattr(args, "output", None) else None

    project_dir, config = find_project_dir(paths)
    if project_dir is not None:
        sources, proj_out = project_sources_and_output(project_dir, config,
                                                       output or Path("a.out"))
        args._project_dir = str(project_dir)
        if not sources:
            raise PolyglotError("No source file found in project at %s"
                                % project_dir)
        for f in sources:
            if not Path(f).exists():
                raise PolyglotError("Source file not found: %s" % f)
        return sources, proj_out, project_dir, config

    if not paths:
        raise PolyglotError("nothing to build", EXIT_USAGE)
    sources = []
    for f in paths:
        f = Path(f)
        if not f.exists():
            raise PolyglotError("Source file not found: %s" % f)
        if f.is_dir():
            raise PolyglotError(
                "Expected source file, got directory: %s (a directory builds "
                "as a project only when it contains a polyglot.toml; otherwise "
                "pass the .c files explicitly: polyglot build %s/*.c)" % (f, f))
        sources.append(f)

    if output is None:
        output = sources[0].with_suffix("") if len(sources) == 1 else Path("a.out")
    return sources, output, Path.cwd(), None


def build_rust_project(tc, sources, output, args):
    rustc = tc.find_tool("rustc")
    if rustc is None:
        raise MissingToolError("rustc", purpose="Rust builds")
    cmd = [str(rustc), RUST_OPT_MAP.get(args.opt_level, "-Copt-level=2")]
    cmd += [str(s) for s in sources] + ["-o", str(output)]
    invoke_compiler(cmd, args)


# --------------------------------------------------------------------------
# Multi-target projects: [[bin]] x N and [[lib]] x N in polyglot.toml
# --------------------------------------------------------------------------

def shared_lib_output(project_dir, name, platform=None):
    """Output path for a [[lib]] kind="shared" target.

    A bare target name gets the lib prefix and the platform's shared-library
    extension ('math' -> 'libmath.so'; an already-suffixed 'libm.so' stays).
    Pure in `platform` so tests can pin every OS.
    """
    out = Path(name)
    plat = sys.platform if platform is None else platform
    suffix = ".dylib" if plat == "darwin" \
        else (".dll" if plat.startswith("win") else ".so")
    if out.suffix in _SHARED_SUFFIXES:
        return project_dir / out
    if not out.name.startswith("lib"):
        out = out.parent / ("lib" + out.name)
    return project_dir / out.with_suffix(suffix)


def project_build_targets(project_dir, config, args, default_output=None):
    """Resolve a multi-target build from polyglot.toml, or None.

    Returns None when the project is the historical single-target shape -
    at most one [[bin]], no [[lib]], no --bin/--lib selection - so cmd_build
    keeps the original single-target code path (byte-identical output).
    Otherwise returns the targets in build order: [[bin]] entries first
    (declaration order), then [[lib]] entries.  Selection is cargo-style:
    any --bin/--lib narrows the build to exactly the named targets, and a
    name not declared in the config raises a friendly exit-2 error.
    """
    bins = parse_bin_entries(config)
    libs = parse_lib_entries(config, project_dir)
    sel_bins = list(getattr(args, "bin_select", None) or [])
    sel_libs = list(getattr(args, "lib_select", None) or [])
    if not sel_bins and not sel_libs and len(bins) <= 1 and not libs:
        return None

    bin_names = [b.get("name") for b in bins]
    for wanted in sel_bins:
        if wanted not in bin_names:
            raise PolyglotError(
                "project has no [[bin]] target named '%s' (targets: %s)"
                % (wanted, ", ".join(n for n in bin_names if n) or "none"),
                EXIT_USAGE)
    lib_names = [lib.get("name") for lib in libs]
    for wanted in sel_libs:
        if wanted not in lib_names:
            raise PolyglotError(
                "project has no [[lib]] target named '%s' (targets: %s)"
                % (wanted, ", ".join(n for n in lib_names if n) or "none"),
                EXIT_USAGE)

    declared = [(project_dir / e["source"]) if e.get("source") else None
                for e in bins]
    # cargo-style selection: any --bin/--lib narrows the build to exactly
    # the named targets; without one, every target builds in order.
    selecting = bool(sel_bins or sel_libs)
    targets = []
    for i, entry in enumerate(bins):
        if selecting and entry.get("name") not in sel_bins:
            continue
        others = [d for j, d in enumerate(declared) if j != i and d is not None]
        sources = resolve_bin_sources(project_dir, declared[i], others)
        name = entry.get("name")
        if not sources:
            raise PolyglotError(
                "no source file found for [[bin]] '%s' in project at %s"
                % (name or "<unnamed>", project_dir))
        output = (project_dir / name) if name \
            else Path(default_output or "a.out")
        targets.append({"kind": "bin", "name": name, "sources": sources,
                        "output": output})
    for lib in libs:
        if selecting and lib["name"] not in sel_libs:
            continue
        if lib["kind"] == "shared":
            output = shared_lib_output(project_dir, lib["name"])
        else:
            output = project_dir / lib["name"]
        targets.append({"kind": lib["kind"], "name": lib["name"],
                        "sources": list(lib["sources"]), "output": output})

    for target in targets:
        for f in target["sources"]:
            if not Path(f).exists():
                raise PolyglotError("Source file not found: %s" % f)

    # a lone selected target still honors -o, like the single-target build;
    # with several targets the per-target names from polyglot.toml win
    if getattr(args, "output", None):
        if len(targets) == 1:
            targets[0]["output"] = Path(args.output)
        else:
            err_print("warning: -o %s ignored: this project builds %d "
                      "targets; use --bin/--lib to select one"
                      % (args.output, len(targets)))
    return targets


def _copy_args(args, **overrides):
    """A shallow args copy with per-target kind flags overridden."""
    lib_args = copy.copy(args)
    for key, value in overrides.items():
        setattr(lib_args, key, value)
    return lib_args


def _build_bin_target(tc, target, args, plugin):
    """One [[bin]] target: plugin path when available, else the direct
    compiler (mirrors the single-target build, one Built line)."""
    sources, output = target["sources"], target["output"]
    if plugin is not None:
        try:
            build_with_plugin(tc, plugin, sources, output, args)
            return
        except PluginError as exc:
            if getattr(args, "plugin", None):
                raise          # an explicit --plugin is mandatory
            err_print("warning: plugin '%s' unusable (%s); falling back to "
                      "the direct compiler" % (Path(plugin).name, exc))
    extra = common_compile_args(tc, args, sources)
    compile_to_exe(tc, sources, output, args, extra)
    if not getattr(args, "static_lib", False):
        print("Built %s -> %s" % (", ".join(str(s) for s in sources), output))


def _build_lib_target(tc, target, args):
    """One [[lib]] target: kind="static" reuses the ar rcs machinery,
    kind="shared" links with -fPIC -shared."""
    sources, output = target["sources"], target["output"]
    extra = common_compile_args(tc, args, sources)
    if target["kind"] == "static":
        # compile_to_exe's static branch archives via ar rcs and prints
        # "Built static library <path>" itself
        compile_to_exe(tc, sources, output, _copy_args(args, shared=False,
                                                       static_lib=True), extra)
    else:
        compile_to_exe(tc, sources, output, _copy_args(args, shared=True,
                                                       static_lib=False), extra)
        print("Built shared library %s" % output)


def build_project_targets(tc, config, targets, args):
    """Build every target in declaration order, one Built line each."""
    # discovery was deferred so polyglot.toml's [compiler] cc/cxx could take
    # effect first; run it now (no-op when the config already supplied one)
    tc.ensure_discovered()
    plugin = None
    bin_targets = [t for t in targets if t["kind"] == "bin"]
    if bin_targets:
        all_sources = []
        for t in bin_targets:
            for s in t["sources"]:
                if s not in all_sources:
                    all_sources.append(s)
        plugin = choose_plugin(args, config, all_sources)
    for target in targets:
        if target["kind"] == "bin":
            _build_bin_target(tc, target, args, plugin)
        else:
            _build_lib_target(tc, target, args)


@command("build", defer=True)
def cmd_build(tc, args):
    paths = [Path(p) for p in (args.paths or ["."])]
    project_dir, config = find_project_dir(paths)
    targets = None
    if project_dir is not None and config \
            and not (config.get("language") or {}).get("rust"):
        targets = project_build_targets(
            project_dir, config, args,
            default_output=Path(getattr(args, "output", None) or "a.out"))
    if targets is not None:
        args._project_dir = str(project_dir)
        tc.apply_project_config(config)
        build_project_targets(tc, config, targets, args)
        return

    sources, output, project_dir, config = resolve_build_inputs(args)
    if config:
        tc.apply_project_config(config)
    if config and (config.get("language") or {}).get("rust"):
        build_rust_project(tc, sources, output, args)
        return
    # discovery was deferred so polyglot.toml's [compiler] cc/cxx could take
    # effect first; run it now (no-op when the config already supplied one)
    tc.ensure_discovered()
    plugin = choose_plugin(args, config, sources)
    if plugin is not None:
        try:
            build_with_plugin(tc, plugin, sources, output, args)
            return
        except PluginError as exc:
            if getattr(args, "plugin", None):
                raise          # an explicit --plugin is mandatory
            err_print("warning: plugin '%s' unusable (%s); falling back to "
                      "the direct compiler" % (Path(plugin).name, exc))
    extra = common_compile_args(tc, args, sources)
    compile_to_exe(tc, sources, output, args, extra)
    if not getattr(args, "static_lib", False):
        print("Built %s -> %s" % (", ".join(str(s) for s in sources), output))


def register(subparsers, global_flags):
    p = subparsers.add_parser("build", help="Build a source file or project")
    p.add_argument("paths", nargs="*", help="source files or project directory")
    p.add_argument("-o", "--output", default=argparse.SUPPRESS, help="output file")
    global_flags.add_common_flags(p, link=True)
    p.add_argument("--shared", action="store_true", help="build a shared library")
    p.add_argument("--static-lib", action="store_true",
                   help="build a static library (ar rcs)")
    p.add_argument("--bin", dest="bin_select", action="append", metavar="NAME",
                   help="build only the [[bin]] target NAME (repeatable; "
                        "project builds)")
    p.add_argument("--lib", dest="lib_select", action="append", metavar="NAME",
                   help="build only the [[lib]] target NAME (repeatable; "
                        "project builds)")
    p.add_argument("-j", "--jobs", type=int, default=1,
                   help="parallel per-TU compilation")
    p.add_argument("--no-cache", action="store_true",
                   help="bypass the incremental object cache")
