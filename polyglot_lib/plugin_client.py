"""Plugin architecture: JSON-RPC 2.0 over stdio, plugin resolution, and the
plugin build path (compile every TU through the plugin, link the IR).

A plugin is an executable speaking the protocol implemented by
crates/polyglot-protocol: newline-delimited JSON-RPC 2.0 on stdin/stdout
(one object per line), human logs on stderr.  Methods: initialize,
compile, shutdown (calls) and exit (notification).  See
docs/user-guide/commands.md "Plugin architecture".
"""

import json
import os
import queue
import shutil
import subprocess
import tempfile
import threading
import time
from pathlib import Path

from polyglot_lib import SCRIPT_DIR
from polyglot_lib.backends import common_compile_args, invoke_compiler, link_args
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_MISSING_TOOL, MissingToolError, PolyglotError,
    err_print, yellow,
)
from polyglot_lib.toolchain import (
    CLANG_ONLY_NAMES, CLANGXX_ONLY_NAMES, CXX_SUFFIXES, Toolchain,
    host_machine, host_os,
)

# Keep in sync with crates/polyglot-protocol/src/types.rs PROTOCOL_VERSION.
PLUGIN_PROTOCOL_VERSION = "0.1.0"

# driver --opt-level name -> protocol OptLevel (serde lowercase spelling)
PLUGIN_OPT_LEVEL = {
    "none": "none", "less": "less", "default": "default",
    "aggressive": "aggressive", "size": "size",
    "size-aggressive": "sizeaggressive",
}

C_CPP_SUFFIXES = {".c", ".h", ".cpp", ".cc", ".cxx", ".hpp", ".c++"}


class PluginError(PolyglotError):
    """A plugin could not be found, executed, spoken to, or trusted."""


class PluginClient:
    """Minimal JSON-RPC 2.0 client for one plugin process over stdio.

    Requests are written to the plugin's stdin, responses read from its
    stdout, one JSON object per line; the plugin's stderr is inherited so
    its diagnostics reach the user directly.  A background thread drains
    stdout so every request carries a deadline, and stray non-JSON lines
    on the stream are skipped rather than fatal.
    """

    def __init__(self, command, timeout=120.0):
        self.command = str(command)
        self.timeout = timeout
        self._next_id = 1
        self._lines = queue.Queue()
        try:
            self.proc = subprocess.Popen(
                [self.command],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=None,
                text=True, bufsize=1)
        except OSError as exc:
            raise PluginError("cannot execute plugin %s: %s" % (command, exc),
                              EXIT_MISSING_TOOL)
        self._reader = threading.Thread(target=self._read_loop, daemon=True)
        self._reader.start()

    def _read_loop(self):
        """Drain the plugin's stdout into a queue (None = EOF sentinel)."""
        while True:
            line = self.proc.stdout.readline()
            self._lines.put(line.rstrip("\n") if line else None)
            if not line:
                return

    def _recv(self, want_id, timeout):
        deadline = time.time() + timeout
        while True:
            remaining = deadline - time.time()
            if remaining <= 0:
                raise PluginError("plugin %s did not respond within %ss"
                                  % (self.command, int(timeout)))
            try:
                line = self._lines.get(timeout=remaining)
            except queue.Empty:
                continue
            if line is None:
                raise PluginError(
                    "plugin %s closed its output before answering "
                    "(is it a polyglot plugin?)" % self.command)
            try:
                msg = json.loads(line)
            except ValueError:
                continue          # tolerate stray non-JSON stream noise
            if not isinstance(msg, dict) or msg.get("id") != want_id:
                continue          # notification or unrelated response
            if "error" in msg:
                rpc_err = msg.get("error") or {}
                raise PluginError("plugin error %s: %s"
                                  % (rpc_err.get("code"),
                                     rpc_err.get("message", "")))
            return msg.get("result")

    def request(self, method, params=None, timeout=None):
        rid = self._next_id
        self._next_id += 1
        req = {"jsonrpc": "2.0", "id": rid, "method": method}
        if params is not None:
            req["params"] = params
        try:
            self.proc.stdin.write(json.dumps(req) + "\n")
            self.proc.stdin.flush()
        except (BrokenPipeError, OSError, ValueError) as exc:
            raise PluginError("plugin %s stopped accepting requests: %s"
                              % (self.command, exc))
        return self._recv(rid, timeout or self.timeout)

    def initialize(self):
        """Handshake; returns the plugin's InitializeResult dict."""
        result = self.request("initialize", {
            "protocol_version": PLUGIN_PROTOCOL_VERSION,
            "capabilities": {"mlir_dialects": [], "source_extensions": [],
                             "languages": [], "incremental": False},
        }, timeout=30.0)
        version = (result or {}).get("protocol_version")
        if not isinstance(result, dict) or version != PLUGIN_PROTOCOL_VERSION:
            raise PluginError(
                "plugin %s speaks protocol %r but this driver expects %r\n"
                "Hint: rebuild the plugin: cargo build --release -p polyglot-clang"
                % (self.command, version, PLUGIN_PROTOCOL_VERSION),
                EXIT_MISSING_TOOL)
        return result

    def compile(self, params):
        """Compile one translation unit; returns the CompileResult dict."""
        return self.request("compile", params)

    def close(self):
        """shutdown call + exit notification, then reap the process."""
        try:
            self.request("shutdown", timeout=10.0)
        except Exception:
            pass
        try:
            self.proc.stdin.write(
                json.dumps({"jsonrpc": "2.0", "method": "exit"}) + "\n")
            self.proc.stdin.flush()
        except (BrokenPipeError, OSError, ValueError):
            pass
        try:
            self.proc.stdin.close()
        except (OSError, AttributeError):
            pass
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pass


def resolve_plugin(name, plugin_dir):
    """Find a plugin binary; returns a Path or None.

    Resolution order: an explicit path when NAME contains a path separator,
    then NAME (and polyglot-NAME, the manifest's command naming) directly
    under --plugin-dir, then inside an installed plugin directory
    (<plugin-dir>/<name>/ and ~/.polyglot/plugins/<name>/, where
    'plugin install' puts binary + manifest), then the workspace
    target/release directory, then PATH.
    """
    if "/" in name or "\\" in name:
        p = Path(name).expanduser()
        return p if (p.is_file() and os.access(p, os.X_OK)) else None
    names = [name]
    if not name.startswith("polyglot-"):
        names.append("polyglot-" + name)
    plugin_root = Path(plugin_dir).expanduser()
    installed_root = Path("~/.polyglot/plugins").expanduser()
    roots = [plugin_root]
    for base in (plugin_root, installed_root):
        for n in names:
            sub = base / n
            if sub not in roots:
                roots.append(sub)
    script_dir = SCRIPT_DIR
    for base in (script_dir / "target" / "release",
                 Path.cwd() / "target" / "release"):
        if base not in roots:
            roots.append(base)
    for root in roots:
        for n in names:
            for cand in (root / n, root / (n + ".exe")):
                try:
                    if cand.is_file() and os.access(cand, os.X_OK):
                        return cand
                except OSError:
                    continue
    for n in names:
        found = shutil.which(n)
        if found:
            return Path(found)
    return None


def plugin_or_missing(name, plugin_dir):
    """Resolve a mandatory --plugin value or fail with exit 3."""
    plugin = resolve_plugin(name, plugin_dir)
    if plugin is None:
        raise PluginError(
            "plugin '%s' not found (searched --plugin-dir, target/release, "
            "PATH)\nHint: build it with: cargo build --release -p "
            "polyglot-clang\nor pass an explicit path: --plugin "
            "/path/to/polyglot-clang" % name, EXIT_MISSING_TOOL)
    return plugin


def choose_plugin(args, config, sources):
    """Which plugin binary should build these sources, or None.

    An explicit --plugin is mandatory: when it is missing or cannot serve
    the languages involved the build fails (exit 3) or falls back with a
    warning for non-C/C++ sources.  Plugins listed in polyglot.toml
    [plugins] are best-effort: a missing one warns and the build falls
    back to the direct compiler.
    """
    plugin_dir = getattr(args, "plugin_dir", None) or "~/.polyglot/plugins"
    explicit = getattr(args, "plugin", None)
    if explicit:
        plugin = plugin_or_missing(explicit, plugin_dir)
        if any(Path(s).suffix not in C_CPP_SUFFIXES for s in sources):
            print(yellow("warning: plugin '%s' handles C/C++ sources; using "
                         "the direct compiler" % explicit))
            return None
        return plugin
    wanted = (config or {}).get("plugins") or {}
    if not wanted or not all(Path(s).suffix in C_CPP_SUFFIXES for s in sources):
        return None
    for name in wanted:
        plugin = resolve_plugin(name, plugin_dir)
        if plugin is None:
            err_print("warning: plugin '%s' (polyglot.toml [plugins]) not "
                      "found; falling back to the direct compiler "
                      "(build it with: cargo build --release -p polyglot-%s)"
                      % (name, name))
            continue
        return plugin
    return None


def format_plugin_diagnostics(diags):
    """Render plugin Diagnostic objects the way compilers do."""
    lines = []
    for d in diags or []:
        loc = d.get("location") or {}
        where = ""
        if loc.get("file"):
            where = "%s:%s:%s: " % (loc.get("file"), loc.get("line", 0),
                                    loc.get("column", 0))
        lines.append("%s%s: %s" % (where, d.get("severity", "error"),
                                   d.get("message", "")))
    return "\n".join(lines)


def plugin_link_cc(tc, sources):
    """A clang able to turn plugin-emitted LLVM IR into machine code.

    C++ sources must link via clang++ (or clang driving C++ mode) so the
    C++ runtime is pulled in; `clang file.ll` links as C and leaves
    std:: symbols undefined.  g++ can never be used here: it cannot
    consume LLVM IR.
    """
    if any(Path(s).suffix in CXX_SUFFIXES for s in sources):
        if tc.kind == "clang" and tc.cxx_path:
            return tc.cxx_path
        if tc.cc_path:
            sibling = tc.cc_path.parent / "clang++"
            try:
                if sibling.is_file() and os.access(sibling, os.X_OK):
                    return sibling
            except OSError:
                pass
        found, _ = Toolchain._find_by_names(CLANGXX_ONLY_NAMES)
        if found:
            return found
    if tc.kind == "clang" and tc.cc_path:
        return tc.cc_path
    found, _ = Toolchain._find_by_names(CLANG_ONLY_NAMES)
    return found


def build_with_plugin(tc, plugin, sources, output, args):
    """Compile every TU through the plugin and link the returned IR.

    The clang plugin returns in-memory LLVM IR text (CompileResult
    .mlir_module, emitted via `clang -S -emit-llvm`), so each result is
    written to a temporary .ll and handed back to a real clang for
    codegen + linking.
    """
    name = Path(plugin).name
    flags = [f for f in common_compile_args(tc, args, sources)
             if not f.startswith("--target=")]
    if getattr(args, "shared", False) and "-fPIC" not in flags:
        flags.append("-fPIC")
    target = getattr(args, "target", None) \
        or (host_machine() + "-" + host_os())
    opt = PLUGIN_OPT_LEVEL.get(getattr(args, "opt_level", "default"),
                               "default")

    client = PluginClient(plugin)
    try:
        try:
            info = client.initialize()
        except PluginError as exc:
            exc.exit_code = EXIT_MISSING_TOOL
            raise
        meta = (info.get("plugin_info") or {})
        if getattr(args, "verbose", False):
            print("plugin         %s v%s (protocol %s)"
                  % (meta.get("name", name), meta.get("version", "?"),
                     info.get("protocol_version", "?")))
        try:
            ir_texts = []
            for src in sources:
                result = client.compile({
                    "source_path": str(Path(src).resolve()),
                    "args": flags,
                    "target_triple": target,
                    "opt_level": opt,
                    "diagnostics": True,
                })
                errors = [d for d in (result.get("diagnostics") or [])
                          if d.get("severity") == "error"]
                if errors:
                    raise PluginError(format_plugin_diagnostics(errors),
                                      EXIT_FAILURE)
                ir = result.get("mlir_module")
                if not isinstance(ir, str) or not ir.strip():
                    raise PluginError("plugin returned no IR module for %s"
                                      % src, EXIT_FAILURE)
                ir_texts.append(ir)
        except PluginError as exc:
            raise PluginError("plugin '%s' compilation failed:\n%s"
                              % (name, exc), EXIT_FAILURE)
        except Exception as exc:
            raise PluginError("plugin '%s' failed: %r" % (name, exc))
    finally:
        client.close()

    link_cc = plugin_link_cc(tc, sources)
    if link_cc is None:
        raise MissingToolError("clang",
                               purpose="linking plugin-emitted LLVM IR")
    with tempfile.TemporaryDirectory() as tmpdir:
        ll_files = []
        for src, ir in zip(sources, ir_texts):
            ll = Path(tmpdir) / (Path(src).stem + ".ll")
            ll.write_text(ir)
            ll_files.append(ll)
        if getattr(args, "static_lib", False):
            objects = []
            for ll in ll_files:
                obj = Path(tmpdir) / (ll.stem + ".o")
                invoke_compiler([str(link_cc), "-c", str(ll), "-o", str(obj)],
                                args, what="plugin IR object emission")
                objects.append(obj)
            ar = tc.find_tool("llvm-ar") or shutil.which("ar")
            if ar is None:
                raise MissingToolError("ar or llvm-ar",
                                       purpose="static library builds")
            out_lib = Path(output)
            if out_lib.suffix != ".a":
                if not out_lib.name.startswith("lib"):
                    out_lib = out_lib.parent / ("lib" + out_lib.name)
                out_lib = out_lib.with_suffix(".a")
            invoke_compiler([str(ar), "rcs", str(out_lib)]
                            + [str(o) for o in objects],
                            args, what="archiving")
            print("Built static library %s (via plugin %s)" % (out_lib, name))
            return
        cmd = [str(link_cc)]
        if getattr(args, "shared", False):
            cmd += ["-shared", "-fPIC"]
        cmd += [str(f) for f in ll_files] + link_args(args, tc) \
            + ["-o", str(output)]
        invoke_compiler(cmd, args, what="linking plugin IR")
    print("Built %s -> %s (via plugin %s)"
          % (", ".join(str(s) for s in sources), output, name))
