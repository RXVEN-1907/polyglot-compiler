"""The 'preview' command: a local Compiler Explorer in the browser.

Serves one offline page (polyglot_lib.preview_assets) on 127.0.0.1 and
answers POST /compile by running the driver's in-process pipeline - the
very same building blocks emit-asm / emit-ir / mca use (Toolchain,
common_compile_args, run_cmd) - never a subprocess of ./polyglot.

Contract of the /compile response (always the same JSON shape):
  {"asm": "...", "ir": "...", "mca": "...", "stderr": "...", "ms": N}
Compile errors and missing-tool degradations come back with status 200 and
their text in "stderr"; only malformed requests get 400/404/413.
"""

import json
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse
import webbrowser
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from types import SimpleNamespace

from polyglot_lib import VERSION
from polyglot_lib.backends import OPT_MAP, common_compile_args
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_USAGE, PolyglotError, quote, run_cmd,
)
from polyglot_lib.preview_assets import DEFAULT_SOURCE, render_page
from polyglot_lib.toolchain import CXX_SUFFIXES, Toolchain

ORDER = 155

#: largest accepted source, in bytes (UTF-8 encoded).
MAX_SOURCE_BYTES = 1 << 20          # 1 MiB
#: largest accepted request body (JSON escaping can roughly double size).
MAX_BODY_BYTES = (2 << 20) + 64 * 1024
#: per compiler-invocation timeout, in seconds.
COMPILE_TIMEOUT = 30
LANG_SUFFIX = {"c": ".c", "cpp": ".cpp"}
BACKENDS = ("auto", "clang", "gcc")

# Shared mutable state (per-backend Toolchain cache); every access is
# serialized through this lock, so concurrent /compile requests never race
# on discovery or on the aux-tool cache warm-up.
_STATE_LOCK = threading.Lock()
_TC_CACHE = {}


class _Proc:
    """Stand-in for subprocess.CompletedProcess after timeouts/OSErrors."""

    def __init__(self, returncode, stdout="", stderr=""):
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr


def _toolchain_for(prefer):
    """Toolchain for a backend select value ("auto"|"clang"|"gcc"), cached.

    Discovery and the llvm-mca warm-up happen under the state lock; after
    that the instance is read-only, so concurrent compiles are safe.
    """
    with _STATE_LOCK:
        tc = _TC_CACHE.get(prefer)
        if tc is None:
            tc = Toolchain(prefer=prefer, require=False)
            tc.find_tool("llvm-mca")     # warm the aux-tool cache once
            _TC_CACHE[prefer] = tc
        return tc


def _preview_args(opt_level):
    """Args namespace shaped for common_compile_args.

    The preview compiles one self-contained snippet: no project flags,
    no include paths, driver-default warning level (which adds no flags).
    """
    return SimpleNamespace(
        std=None, warn="default", werror=False, debug=False, sanitize=None,
        includes=[], defines=[], cc_args=[], opt_level=opt_level, target=None,
        quiet=True, verbose=False)


def _run(cmd, **kwargs):
    """run_cmd with output capture and the per-invocation timeout; never
    raises (timeouts and spawn failures become failed _Proc results)."""
    try:
        return run_cmd(cmd, capture_output=True, text=True,
                       timeout=COMPILE_TIMEOUT, **kwargs)
    except subprocess.TimeoutExpired:
        return _Proc(-1, "", "timed out after %ds: %s"
                     % (COMPILE_TIMEOUT, quote(cmd)))
    except OSError as exc:
        return _Proc(-1, "", "failed to run %s: %s" % (Path(str(cmd[0])).name,
                                                       exc))


def _read_file(path, default=""):
    try:
        return Path(path).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return default


def _diagnostics(proc, label=None):
    """Combined compiler chatter for one stage, optionally headed."""
    text = ((proc.stdout or "") + (proc.stderr or "")).strip()
    if label and text:
        text = "--- %s ---\n%s" % (label, text)
    return text


def _join_diag(parts):
    return "\n".join(p for p in parts if p)


def compile_snippet(source, language="c", opt_level="default",
                    backend="auto", intel=False):
    """Compile one in-memory source through the in-process pipeline.

    Returns {"asm", "ir", "mca", "stderr", "ms"}; raises PolyglotError
    (exit code 2) only for invalid request fields or an oversized source.
    """
    started = time.monotonic()

    def result(asm="", ir="", mca="", stderr=""):
        return {"asm": asm, "ir": ir, "mca": mca, "stderr": stderr,
                "ms": int((time.monotonic() - started) * 1000)}

    if not isinstance(source, str):
        raise PolyglotError("'source' must be a string", EXIT_USAGE)
    if language not in LANG_SUFFIX:
        raise PolyglotError("invalid language %r (choose from: %s)"
                            % (language, ", ".join(sorted(LANG_SUFFIX))),
                            EXIT_USAGE)
    if opt_level not in OPT_MAP:
        raise PolyglotError("invalid opt level %r (choose from: %s)"
                            % (opt_level, ", ".join(OPT_MAP)), EXIT_USAGE)
    if backend not in BACKENDS:
        raise PolyglotError("invalid backend %r (choose from: %s)"
                            % (backend, ", ".join(BACKENDS)), EXIT_USAGE)
    raw = source.encode("utf-8", errors="replace")
    if len(raw) > MAX_SOURCE_BYTES:
        raise PolyglotError("source is %d bytes; the preview limit is %d "
                            "bytes" % (len(raw), MAX_SOURCE_BYTES), EXIT_USAGE)

    try:
        tc = _toolchain_for(backend)
    except PolyglotError as exc:
        return result(stderr="error: %s" % exc)
    if tc.cc_path is None:
        return result(stderr="error: no C compiler found (install clang/gcc "
                             "or set POLYGLOT_CC)")

    args = _preview_args(opt_level)
    with tempfile.TemporaryDirectory(prefix="polyglot_preview_") as tmpdir:
        root = Path(tmpdir)
        src = root / ("snippet" + LANG_SUFFIX[language])
        src.write_bytes(raw)

        try:
            compiler = tc.compiler_for([src])
        except PolyglotError as exc:
            return result(stderr="error: %s" % exc)
        if compiler is None:
            return result(stderr="error: no C compiler found (install "
                                 "clang/gcc or set POLYGLOT_CC)")

        base = common_compile_args(tc, args, [src])

        # assembly ----------------------------------------------------------
        asm_flags = list(base)
        if intel:
            asm_flags.append("-masm=intel")
        asm_path = root / "snippet.s"
        asm_proc = _run([compiler] + asm_flags
                        + ["-S", str(src), "-o", str(asm_path)])
        if asm_proc.returncode != 0:
            return result(stderr=_diagnostics(asm_proc))
        asm = _read_file(asm_path)
        diag = _diagnostics(asm_proc)

        # IR: clang -> LLVM IR, gcc -> GIMPLE dump (as in emit-ir) -----------
        ir = ""
        ir_label = "GIMPLE emission" if tc.kind == "gcc" else "IR emission"
        if tc.kind == "clang":
            ir_path = root / "snippet.ll"
            ir_proc = _run([compiler]
                           + common_compile_args(tc, args, [src], for_emit=True)
                           + ["-S", "-emit-llvm", str(src), "-o", str(ir_path)])
            produced = ir_path if ir_proc.returncode == 0 else None
        else:
            gdir = root / "gimple"
            gdir.mkdir()
            ir_proc = _run([compiler] + base
                           + ["-fdump-tree-gimple", "-S", str(src.resolve()),
                              "-o", str(gdir / "out")],
                           cwd=str(gdir))
            dumps = sorted(gdir.glob("*.gimple")) \
                if ir_proc.returncode == 0 else []
            produced = dumps[-1] if dumps else None
        if produced is not None:
            ir = _read_file(produced)
        else:
            diag = _join_diag([diag, _diagnostics(ir_proc, label=ir_label)])

        # machine-code analysis ----------------------------------------------
        mca = ""
        mca_bin = tc.find_tool("llvm-mca")
        if mca_bin is None:
            mca = "# llvm-mca not found (install LLVM or extend PATH)"
        else:
            mca_proc = _run([mca_bin, "--iterations=3", str(asm_path)])
            if mca_proc.returncode != 0:
                mca = "# llvm-mca failed:\n" + ((mca_proc.stderr or "")
                                                + (mca_proc.stdout or "")).strip()
            else:
                mca = mca_proc.stdout or ""

        return result(asm=asm, ir=ir, mca=mca, stderr=diag)


def _initial_state(args):
    """(source, language) the editor starts with: the FILE argument's
    contents (language inferred from the suffix) or the demo snippet."""
    path = getattr(args, "file", None)
    if not path:
        return DEFAULT_SOURCE, "c"
    p = Path(path)
    if not p.exists():
        raise PolyglotError("file not found: %s" % p)
    data = p.read_bytes()
    if len(data) > MAX_SOURCE_BYTES:
        raise PolyglotError("file %s is %d bytes; the preview limit is %d "
                            "bytes" % (p, len(data), MAX_SOURCE_BYTES),
                            EXIT_USAGE)
    source = data.decode("utf-8", errors="replace")
    language = "cpp" if p.suffix in CXX_SUFFIXES else "c"
    return source, language


class _PreviewServer(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True

    def handle_error(self, request, client_address):
        """A client vanishing mid-request is routine; stay quiet, and keep
        the server console free of tracebacks."""
        exc = sys.exc_info()[1]
        if isinstance(exc, (BrokenPipeError, ConnectionResetError,
                            TimeoutError)):
            return
        print("[preview] request from %s failed: %r"
              % (client_address[0], exc), flush=True)


class _PreviewHandler(BaseHTTPRequestHandler):
    server_version = "polyglot-preview/%s" % VERSION
    protocol_version = "HTTP/1.1"
    page = b""     # set per instance class by _make_handler()

    def log_message(self, fmt, *args):
        print("[preview] %s %s" % (self.address_string(), fmt % args),
              flush=True)

    # -- senders -----------------------------------------------------------

    def _send_json(self, obj, status=200, close=False):
        body = json.dumps(obj).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        if close:
            self.send_header("Connection", "close")
            self.close_connection = True
        self.end_headers()
        self.wfile.write(body)

    def _send_compile_error(self, message, status):
        self._send_json({"asm": "", "ir": "", "mca": "", "stderr": message,
                         "ms": 0}, status=status)

    # -- routes --------------------------------------------------------------

    def do_GET(self):
        path = urllib.parse.urlsplit(self.path).path
        if path in ("", "/", "/index.html"):
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(self.page)))
            self.end_headers()
            self.wfile.write(self.page)
        elif path == "/health":
            self._send_json({"ok": True, "version": VERSION})
        else:
            self._send_json({"error": "not found: %s" % path}, status=404,
                            close=True)

    def do_POST(self):
        path = urllib.parse.urlsplit(self.path).path
        if path != "/compile":
            self._send_json({"error": "not found: %s" % path}, status=404,
                            close=True)
            return
        try:
            length = int(self.headers.get("Content-Length") or 0)
        except ValueError:
            length = 0
        if length <= 0:
            self._send_compile_error("empty or chunked request body; a "
                                     "Content-Length is required", 400)
            return
        if length > MAX_BODY_BYTES:
            self._send_compile_error("request body too large", 413)
            return
        raw = self.rfile.read(length)
        try:
            payload = json.loads(raw.decode("utf-8", errors="replace"))
        except ValueError:
            self._send_compile_error("request body is not valid JSON", 400)
            return
        if not isinstance(payload, dict):
            self._send_compile_error("request body must be a JSON object",
                                     400)
            return
        source = payload.get("source")
        if not isinstance(source, str):
            self._send_compile_error("'source' must be a string", 400)
            return
        try:
            result = compile_snippet(
                source,
                language=payload.get("language", "c"),
                opt_level=payload.get("opt_level", "default"),
                backend=payload.get("backend", "auto"),
                intel=bool(payload.get("intel", False)))
        except PolyglotError as exc:
            self._send_compile_error(str(exc), 400)
            return
        self._send_json(result)


def _make_handler(page_bytes):
    """Handler class bound to one rendered page."""
    handler = type("PreviewHandler", (_PreviewHandler,), {"page": page_bytes})
    return handler


def _open_browser(url):
    try:
        webbrowser.open(url)
    except Exception:            # browser probing must never kill the server
        pass


@command("preview", toolchain=False)
def cmd_preview(tc, args):
    source, language = _initial_state(args)
    handler = _make_handler(render_page(source, language).encode("utf-8"))
    try:
        server = _PreviewServer(("127.0.0.1", args.port), handler)
    except OSError as exc:
        raise PolyglotError("cannot bind 127.0.0.1:%d (%s)"
                            % (args.port, exc), EXIT_FAILURE)
    url = "http://127.0.0.1:%d/" % server.server_address[1]
    print("polyglot preview %s" % VERSION)
    print("url: %s" % url, flush=True)
    print("press Ctrl+C to stop")
    if not getattr(args, "no_browser", False):
        threading.Timer(0.5, _open_browser, (url,)).start()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nstopped.")
    finally:
        server.server_close()


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "preview",
        help="Local Compiler Explorer: edit and compile in a browser tab",
        description="Serve a local Compiler-Explorer page on 127.0.0.1: "
                    "edit a snippet, compile it with the driver's "
                    "in-process pipeline, and inspect the assembly, "
                    "LLVM IR/GIMPLE and llvm-mca output in browser tabs.")
    p.add_argument("file", nargs="?",
                   help="source file to prefill the editor (default: demo "
                        "snippet; language inferred from the suffix)")
    p.add_argument("--port", type=int, default=8641,
                   help="TCP port to listen on, bound to 127.0.0.1 only "
                        "(default: 8641; 0 picks a free port)")
    p.add_argument("--no-browser", action="store_true",
                   help="do not open the browser automatically")
