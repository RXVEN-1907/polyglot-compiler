"""Backend-aware flag translation (clang / GCC, plus rustc opt levels) and
the core compile/link machinery shared by the build-like commands."""

import os
import shutil
import subprocess
import tempfile
import threading
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from polyglot_lib.cache import BuildCache
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_USAGE, MissingToolError, PolyglotError,
    log_running, run_cmd, yellow,
)
from polyglot_lib.toolchain import CXX_SUFFIXES

OPT_MAP = {
    "none": "-O0",
    "less": "-O1",
    "default": "-O2",
    "aggressive": "-O3",
    "size": "-Os",
    "size-aggressive": "-Oz",
}

RUST_OPT_MAP = {
    "none": "-Copt-level=0",
    "less": "-Copt-level=1",
    "default": "-Copt-level=2",
    "aggressive": "-Copt-level=3",
    "size": "-Copt-level=z",
    "size-aggressive": "-Copt-level=z",
}

C_STDS = ["c89", "c99", "c11", "c17", "c2x", "gnu89", "gnu99", "gnu11", "gnu17"]
CXX_STDS = ["c++98", "c++11", "c++14", "c++17", "c++20", "c++23", "gnu++98",
            "gnu++11", "gnu++14", "gnu++17", "gnu++20"]
SANITIZERS = ["address", "undefined", "thread", "memory", "leak"]

WARN_MAP = {
    "none": [],
    "default": [],
    "all": ["-Wall"],
    "extra": ["-Wall", "-Wextra"],
    "pedantic": ["-Wall", "-Wextra", "-Wpedantic"],
}


# --------------------------------------------------------------------------
# Flag construction (backend-aware)
# --------------------------------------------------------------------------

def validate_std(std, source_files):
    """Validate --std against the language implied by the source suffix."""
    if not std:
        return None
    is_cxx = any(Path(f).suffix in CXX_SUFFIXES for f in source_files)
    allowed = CXX_STDS if is_cxx else C_STDS
    if std not in allowed:
        lang = "C++" if is_cxx else "C"
        raise PolyglotError(
            "invalid --std '%s' for %s sources (choose from: %s)"
            % (std, lang, ", ".join(allowed)), EXIT_USAGE)
    return std


def default_std(source_files):
    """Per-suffix default standard when the user did not ask for one."""
    return "c++17" if any(Path(f).suffix in CXX_SUFFIXES for f in source_files) \
        else "c17"


def common_compile_args(tc, args, source_files, *, for_emit=False):
    """Build the shared flag list honored by both clang and GCC."""
    flags = []
    std = validate_std(getattr(args, "std", None), source_files) \
        or default_std(source_files)
    flags.append("-std=" + std)

    warn = getattr(args, "warn", "default") or "default"
    flags.extend(WARN_MAP.get(warn, WARN_MAP["default"]))
    if getattr(args, "werror", False):
        flags.append("-Werror")
    if getattr(args, "debug", False):
        flags.append("-g")

    sanitizers = getattr(args, "sanitize", None)
    if sanitizers:
        chosen = [s.strip() for s in sanitizers.split(",") if s.strip()]
        for s in chosen:
            if s not in SANITIZERS:
                raise PolyglotError(
                    "invalid sanitizer '%s' (choose from: %s)"
                    % (s, ", ".join(SANITIZERS)), EXIT_USAGE)
            if s == "memory" and tc.kind == "gcc":
                raise PolyglotError(
                    "-fsanitize=memory is clang-only; the GCC backend does not "
                    "support MemorySanitizer (use clang or drop 'memory').",
                    EXIT_USAGE)
        if chosen:
            flags.append("-fsanitize=" + ",".join(chosen))

    for inc in getattr(args, "includes", None) or []:
        flags.extend(["-I", str(inc)])
    for define in getattr(args, "defines", None) or []:
        flags.append("-D" + define)

    opt = OPT_MAP.get(getattr(args, "opt_level", "default"), "-O2")
    if opt == "-Oz" and tc.kind == "gcc":
        # -Oz needs gcc >= 12; -Os is valid everywhere and semantically
        # closest (mirrors the -fsanitize=memory backend guard above).
        opt = "-Os"
    flags.append(opt)

    target = getattr(args, "target", None)
    if target and tc.kind == "clang":
        flags.append("--target=" + target)
    else:
        tc.needs_target_ok(target)

    if for_emit and tc.kind == "clang":
        flags.append("-fno-discard-value-names")

    # User-supplied raw flags go last: compilers honor the final -O/-f...,
    # so an explicit --cc-arg=-O0 must win over the driver's default.
    for raw in getattr(args, "cc_args", None) or []:
        flags.append(raw)
    return flags


def link_args(args, tc):
    """Library search paths / libraries for the link step."""
    flags = []
    for libdir in getattr(args, "lib_dirs", None) or []:
        flags.extend(["-L", str(libdir)])
    for lib in getattr(args, "libs", None) or []:
        flags.extend(["-l", str(lib)])
    return flags


_PATH_FLAG_PAIRS = ("I", "isystem", "iquote", "include", "idirafter", "L")


def _absolutize_path_flags(flags, base):
    """Rewrite path-bearing flags to absolute form (against `base`).

    Needed because the GCC emit-ir path runs the compiler inside a temporary
    directory: relative -I/-include/-L values would otherwise resolve there
    and break the build.
    """
    out = []
    i = 0
    while i < len(flags):
        f = str(flags[i])
        if f in ["-%s" % n for n in _PATH_FLAG_PAIRS] and i + 1 < len(flags):
            p = Path(flags[i + 1])
            out.append(f)
            out.append(str(p if p.is_absolute() else (base / p).resolve()))
            i += 2
            continue
        if len(f) > 2 and f[0] == "-" and f[1] in "IL" and f[2] != "=":
            p = Path(f[2:])
            if not p.is_absolute():
                out.append(f[:2] + str((base / p).resolve()))
                i += 1
                continue
        out.append(f)
        i += 1
    return out


# --------------------------------------------------------------------------
# Core compile / link machinery
# --------------------------------------------------------------------------

def invoke_compiler(cmd, args, what="compilation"):
    log_running(cmd, args)
    proc = run_cmd(cmd)
    if proc.returncode != 0:
        raise PolyglotError("%s failed with exit code %d"
                            % (what, proc.returncode), proc.returncode or EXIT_FAILURE)
    return proc


def compile_object(tc, source, out_obj, args, extra_flags):
    """One translation unit -> one .o file."""
    compiler = tc.compiler_for([source])
    cmd = [compiler] + extra_flags + ["-c", str(source), "-o", str(out_obj)]
    invoke_compiler(cmd, args)


def compile_to_exe(tc, sources, output, args, extra_flags, for_project=False):
    """Build executable (or shared/static lib) from sources.

    Single TU with --no-cache -> one direct compiler invocation.
    Otherwise every TU is compiled to a (cached) .o file - optionally in
    parallel via -j - and then linked by one final compiler invocation.
    """
    shared = getattr(args, "shared", False)
    static_lib = getattr(args, "static_lib", False)

    project_dir = Path(getattr(args, "_project_dir", None) or Path.cwd())
    cache = BuildCache(project_dir, enabled=not getattr(args, "no_cache", False))
    if cache.enabled:
        try:
            cache.dir.mkdir(parents=True, exist_ok=True)
        except OSError:
            cache.enabled = False

    def one(src):
        flags = list(extra_flags)
        if shared and "-fPIC" not in flags:
            # position-independent code is required for every TU of a
            # .so, not just at the link step
            flags.append("-fPIC")
        key = BuildCache.key(tc, flags, src, project_dir)
        cached = cache.get(key)
        if cached is not None:
            if getattr(args, "verbose", False):
                print(yellow("cache HIT  %s" % src))
            return cached
        if getattr(args, "verbose", False):
            print("cache MISS %s" % src)
        if cache.enabled:
            # pid+thread-unique scratch name; store() atomically renames it
            tmp_obj = cache.object_path("%s.%d.%d.tmp"
                                        % (key, os.getpid(),
                                           threading.get_ident()))
        else:
            fd, tmp_name = tempfile.mkstemp(suffix=".o")
            os.close(fd)
            tmp_obj = Path(tmp_name)
            no_cache_temps.append(tmp_obj)
        compile_object(tc, src, tmp_obj, args, flags)
        if cache.enabled:
            cache.store(key, src, flags, tmp_obj)
            return cache.object_path(key)
        return tmp_obj

    no_cache_temps = []
    try:
        if len(sources) == 1 and not cache.enabled and not shared \
                and not static_lib:
            compiler = tc.compiler_for(sources)
            cmd = [compiler] + extra_flags + link_args(args, tc)
            cmd += [str(s) for s in sources] + ["-o", str(output)]
            invoke_compiler(cmd, args)
            return

        workers = max(1, int(getattr(args, "jobs", 1) or 1))
        if len(sources) > 1 and workers > 1:
            with ThreadPoolExecutor(max_workers=workers) as pool:
                objects = list(pool.map(one, sources))
        else:
            objects = [one(s) for s in sources]

        if static_lib:
            ar = tc.find_tool("llvm-ar") or shutil.which("ar")
            if ar is None:
                raise MissingToolError("ar or llvm-ar",
                                       purpose="static library builds")
            out_lib = Path(output)
            if out_lib.suffix != ".a":
                # user gave a bare name: normalize to lib<name>.a
                if not out_lib.name.startswith("lib"):
                    out_lib = out_lib.parent / ("lib" + out_lib.name)
                out_lib = out_lib.with_suffix(".a")
            cmd = [str(ar), "rcs", str(out_lib)] + [str(o) for o in objects]
            invoke_compiler(cmd, args, what="archiving")
            print("Built static library %s" % out_lib)
            return

        compiler = tc.compiler_for(sources)
        cmd = [compiler] + extra_flags
        if shared:
            cmd += ["-shared", "-fPIC"]
        cmd += [str(o) for o in objects] + link_args(args, tc) \
            + ["-o", str(output)]
        invoke_compiler(cmd, args, what="linking")
    finally:
        # --no-cache compiles into one-shot mkstemp files: unlink them so
        # repeated builds (e.g. watch mode) do not leak objects into /tmp.
        for t in no_cache_temps:
            try:
                t.unlink(missing_ok=True)
            except OSError:
                pass


__all__ = [
    "OPT_MAP", "RUST_OPT_MAP", "C_STDS", "CXX_STDS", "SANITIZERS", "WARN_MAP",
    "validate_std", "default_std", "common_compile_args", "link_args",
    "invoke_compiler", "compile_object", "compile_to_exe",
]
