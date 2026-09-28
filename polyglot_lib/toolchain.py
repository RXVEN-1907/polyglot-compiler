"""Toolchain discovery: locate the C/C++ compiler (clang or GCC) and the
auxiliary LLVM tools, plus host/target-triple helpers."""

import os
import shutil
import subprocess
import sys
from pathlib import Path

from polyglot_lib.output import (
    EXIT_USAGE, MissingToolError, PolyglotError, run_cmd,
)

CXX_SUFFIXES = {".cpp", ".cc", ".cxx", ".c++", ".C", ".cp", ".m", ".mm"}

# Versioned clang names are generated newest-first so freshly released
# toolchains (clang-20/21, ...) are discovered without code changes.
_CLANG_V = range(21, 14, -1)          # clang-21 ... clang-15
C_COMPILER_NAMES = ["clang"] + ["clang-%d" % v for v in _CLANG_V] + ["gcc", "cc"]
CXX_COMPILER_NAMES = ["clang++"] + ["clang++-%d" % v for v in _CLANG_V] \
    + ["g++", "c++"]
CLANG_ONLY_NAMES = ["clang"] + ["clang-%d" % v for v in _CLANG_V]
GCC_ONLY_NAMES = ["gcc", "cc"]
CLANGXX_ONLY_NAMES = ["clang++"] + ["clang++-%d" % v for v in _CLANG_V]
GCCXX_ONLY_NAMES = ["g++", "c++"]

# Directories scanned (in addition to PATH) when a compiler or auxiliary
# tool cannot be found on PATH.
SCAN_GLOBS = [
    "~/llvm-install/*/bin",
    "/usr/lib/llvm-*/bin",
    "/usr/local/bin",
    "/opt/homebrew/opt/llvm/bin",
    "/opt/homebrew/bin",
    "/usr/local/opt/llvm/bin",
    "/opt/*/bin",
]

# name -> human description, used by doctor / missing-tool messages.
AUX_TOOLS = {
    "clang-format": "source code formatter",
    "clang-tidy": "static linter",
    "clang-repl": "interactive C++ REPL",
    "lli": "LLVM bitcode JIT interpreter",
    "opt": "LLVM optimizer",
    "llc": "LLVM static compiler",
    "llvm-mca": "LLVM machine code analyzer",
    "llvm-nm": "symbol table tool",
    "llvm-size": "binary size tool",
    "llvm-objdump": "object dumper",
    "llvm-ar": "archiver (LLVM)",
    "llvm-cov": "coverage tool",
    "llvm-profdata": "profile data tool",
    "rustc": "Rust compiler",
    "scan-build": "clang static analysis driver",
    "wasm-ld": "WebAssembly linker",
}

KNOWN_TARGETS = [
    ("x86_64-unknown-linux-gnu", "x86-64 Linux (native on this class of host)"),
    ("aarch64-unknown-linux-gnu", "AArch64 Linux"),
    ("arm-unknown-linux-gnueabihf", "ARM 32-bit Linux (hard-float)"),
    ("riscv64-unknown-linux-gnu", "RISC-V 64-bit Linux"),
    ("wasm32-wasip1", "WebAssembly 32-bit WASI preview 1"),
]


def host_machine():
    m = os.uname().machine.lower()
    return {"amd64": "x86_64", "arm64": "aarch64", "armv7l": "arm",
            "armv8l": "arm", "i686": "x86", "i386": "x86"}.get(m, m)


def os_suffix(platform=None):
    """Vendor/OS part of a target triple for the given sys.platform value.

    Parameterized so the mapping stays a pure, testable function:
    linux -> unknown-linux-gnu, darwin -> apple-darwin, windows -> pc-windows-gnu.
    """
    plat = sys.platform if platform is None else platform
    if plat == "darwin":
        return "apple-darwin"
    if plat.startswith("win"):
        return "pc-windows-gnu"
    return "unknown-linux-gnu"


def host_os():
    """Vendor/OS part of the host triple, e.g. for `polyglot init`."""
    return os_suffix()


def host_triple(machine=None, platform=None):
    """Full '<machine>-<os>' triple; pure so tests can pin each platform."""
    m = host_machine() if machine is None else machine
    return "%s-%s" % (m, os_suffix(platform))


def target_matches_host(target):
    """True when a --target triple refers to this host (for GCC silently-ok)."""
    if not target:
        return True
    arch = target.split("-")[0].lower()
    arch = {"amd64": "x86_64", "arm64": "aarch64", "armv7": "arm",
            "armv7l": "arm", "armhf": "arm", "i686": "x86"}.get(arch, arch)
    if arch != host_machine():
        return False
    if "linux" in target and os.uname().sysname.lower() != "linux":
        return False
    if os.uname().sysname.lower() == "linux":
        # same-arch triples for a *different* OS are cross builds too; the
        # host gcc cannot serve them silently.
        for tok in ("darwin", "macos", "mingw", "windows", "freebsd",
                    "netbsd", "openbsd", "solaris", "aix", "haiku"):
            if tok in target:
                return False
    return True


# --------------------------------------------------------------------------
# Toolchain discovery
# --------------------------------------------------------------------------

_MSVC_DRIVER_NAMES = ("cl", "cl.exe")


def reject_msvc(path):
    """Clear unsupported-backend error when POLYGLOT_CC names cl.exe.

    The MSVC driver speaks a different flag dialect entirely (-o, -c and
    the link model do not map), so it must fail fast with an explanation
    instead of a baffling compile error.  Pure so tests can pin it.
    """
    if path is not None and Path(path).name.lower() in _MSVC_DRIVER_NAMES:
        raise PolyglotError(
            "POLYGLOT_CC points to cl.exe (%s): the MSVC driver interface is "
            "not supported.\nHint: point POLYGLOT_CC at a clang or gcc "
            "binary." % path, EXIT_USAGE)
    return path

class Toolchain:
    """Locates the C/C++ compiler (clang or GCC) and auxiliary LLVM tools.

    Precedence for the C compiler:
      1. POLYGLOT_CC / POLYGLOT_CXX environment variables (absolute).
      2. --compiler clang|gcc forced selection on PATH / scan dirs.
      3. Auto discovery: PATH candidates in preference order, then
         well-known LLVM install directories.
    """

    def __init__(self, prefer="auto", defer=False, require=True):
        self.prefer = prefer
        self.cc_path = None
        self.cxx_path = None
        self.kind = None            # "clang" | "gcc"
        self.version = "unknown"
        self.found_off_path = False
        self._aux_cache = {}
        self._require = require
        if not defer:
            self._discover()

    # -- discovery ---------------------------------------------------------

    @staticmethod
    def _scan_dirs():
        dirs = []
        seen = set()
        for pattern in SCAN_GLOBS:
            try:
                # expanduser needs HOME (or a pwd entry); an environment
                # without either must degrade, not crash.
                expanded = Path(pattern).expanduser()
                for d in sorted(Path("/").glob(str(expanded).lstrip("/"))):
                    if d not in seen:
                        seen.add(d)
                        dirs.append(d)
            except (OSError, RuntimeError, ValueError):
                continue
        return dirs

    @staticmethod
    def _find_on_path(names):
        """First PATH hit among names, or None."""
        for name in names:
            p = shutil.which(name)
            if p:
                return Path(p)
        return None

    @classmethod
    def _find_by_names(cls, names):
        """Search PATH first, then well-known scan dirs, in name order."""
        for name in names:
            p = shutil.which(name)
            if p:
                return Path(p), False
        for d in cls._scan_dirs():
            for name in names:
                cand = d / name
                try:
                    if cand.is_file() and os.access(cand, os.X_OK):
                        return cand, True
                except OSError:
                    continue
        return None, False

    def _detect_kind_and_version(self):
        try:
            proc = run_cmd([self.cc_path, "--version"], capture_output=True,
                           text=True, timeout=30)
            out = (proc.stdout or "") + (proc.stderr or "")
        except (OSError, subprocess.SubprocessError) as exc:
            raise PolyglotError("cannot execute compiler %s: %s"
                                % (self.cc_path, exc))
        low = out.lower()
        first = (out.strip().splitlines() or ["unknown"])[0].strip()
        self.version = first
        if "clang" in low:
            self.kind = "clang"
        elif "gcc" in low or "free software foundation" in low:
            self.kind = "gcc"
        else:
            raise PolyglotError(
                "unrecognized compiler backend at %s (use --compiler clang|gcc)"
                % self.cc_path)

    def _discover(self):
        env_cc = os.environ.get("POLYGLOT_CC")

        if self.prefer == "clang":
            names = CLANG_ONLY_NAMES
        elif self.prefer == "gcc":
            names = GCC_ONLY_NAMES
        else:
            names = C_COMPILER_NAMES

        cc, off_path = self._find_by_names(names)
        if env_cc:
            cc, off_path = reject_msvc(Path(env_cc)), False
        if cc is None:
            if self._require:
                raise MissingToolError(
                    "a C compiler (%s)" % "/".join(names[:3] + ["..."]),
                    searched="PATH, " + ", ".join(SCAN_GLOBS),
                    purpose="all polyglot commands")
            self.cc_path = None      # discovery state saved for doctor/version
            self.found_off_path = False
            return
        if not off_path and env_cc is None and self.prefer != "auto":
            # forced selection also accepts scan-dir hits
            pass
        self.cc_path = cc
        self.found_off_path = off_path
        self._detect_kind_and_version()
        self._discover_cxx()

    def _discover_cxx(self):
        """Locate the C++ compiler (same family preferred, other fallback)."""
        if self.cxx_path is not None:
            return
        env_cxx = os.environ.get("POLYGLOT_CXX")
        if env_cxx:
            self.cxx_path = Path(env_cxx)
            return
        if self.prefer == "clang":
            cxx_names = CLANGXX_ONLY_NAMES
        elif self.prefer == "gcc":
            cxx_names = GCCXX_ONLY_NAMES
        elif self.kind == "clang":
            cxx_names = CLANGXX_ONLY_NAMES + GCCXX_ONLY_NAMES
        else:
            cxx_names = GCCXX_ONLY_NAMES + CLANGXX_ONLY_NAMES
        cxx = self._find_on_path(cxx_names)
        if cxx is None and self.cc_path is not None:
            # sibling pass: clang++ sitting next to the discovered clang
            for name in cxx_names:
                cand = self.cc_path.parent / name
                try:
                    if cand.is_file() and os.access(cand, os.X_OK):
                        cxx = cand
                        break
                except OSError:
                    continue
        if cxx is None:
            cxx, _ = self._find_by_names(cxx_names)
        self.cxx_path = cxx

    def ensure_discovered(self):
        """Run discovery lazily (build defers it until polyglot.toml is read).

        When a project's [compiler] section already supplied a C compiler,
        only the still-missing C++ compiler is looked up.
        """
        if self.cc_path is None:
            self._discover()
        else:
            self._discover_cxx()

    # -- auxiliary tools ---------------------------------------------------

    def find_tool(self, name):
        """Locate an auxiliary tool: PATH, then clang's own bin dir, then
        the well-known LLVM scan dirs.  Returns a Path or None."""
        if name in self._aux_cache:
            return self._aux_cache[name]
        found = None
        p = shutil.which(name)
        if p:
            found = Path(p)
        else:
            tried = []
            if self.cc_path:
                sibling = self.cc_path.parent / name
                tried.append(str(sibling))
                try:
                    if sibling.is_file() and os.access(sibling, os.X_OK):
                        found = sibling
                except OSError:
                    pass
            if found is None:
                for d in self._scan_dirs():
                    cand = d / name
                    tried.append(str(cand))
                    try:
                        if cand.is_file() and os.access(cand, os.X_OK):
                            found = cand
                            break
                    except OSError:
                        continue
        self._aux_cache[name] = found
        return found

    def tool_or_error(self, name, purpose):
        p = self.find_tool(name)
        if p is None:
            raise MissingToolError(name, searched="PATH and LLVM install dirs",
                                   purpose=purpose)
        return p

    def compiler_for(self, source_files):
        """Pick cc vs cxx by the suffixes present."""
        if any(Path(f).suffix in CXX_SUFFIXES for f in source_files):
            if self.cxx_path:
                return self.cxx_path
            raise MissingToolError("a C++ compiler (clang++/g++)",
                                   purpose="C++ sources")
        return self.cc_path

    def needs_target_ok(self, target):
        """Validate --target for this backend; raises on unsupported."""
        if not target or self.kind == "clang":
            return
        if not target_matches_host(target):
            raise PolyglotError(
                "GCC backend cannot target '%s': GCC needs a matching cross "
                "toolchain (e.g. aarch64-linux-gnu-gcc).\nHint: use clang for "
                "cross-compilation (--compiler clang)." % target)

    def apply_project_config(self, config):
        """Honour polyglot.toml [compiler] cc/cxx keys for project builds.

        POLYGLOT_CC / POLYGLOT_CXX environment variables keep absolute
        precedence over the config file.
        """
        comp = (config or {}).get("compiler") or {}
        changed = False

        def resolve(value):
            p = Path(value).expanduser()
            if not p.is_absolute():
                p = Path(shutil.which(value) or p)
            return p if p.exists() else None

        cc = comp.get("cc")
        if cc and not os.environ.get("POLYGLOT_CC"):
            p = resolve(cc)
            if p is not None:
                self.cc_path = p
                self.found_off_path = shutil.which("clang") is None \
                    and self.kind == "clang"
                changed = True
            else:
                raise PolyglotError("polyglot.toml [compiler] cc = '%s' does "
                                    "not resolve to an executable" % cc)
        cxx = comp.get("cxx")
        if cxx and not os.environ.get("POLYGLOT_CXX"):
            p = resolve(cxx)
            if p is not None:
                self.cxx_path = p
            else:
                raise PolyglotError("polyglot.toml [compiler] cxx = '%s' does "
                                    "not resolve to an executable" % cxx)
        if changed:
            self._detect_kind_and_version()

    def describe(self):
        return "%s (%s, %s)" % (self.cc_path, self.kind, self.version)
