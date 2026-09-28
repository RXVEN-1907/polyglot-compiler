"""The 'explain-ir' command: IR with each function's source beside it.

Emits IR through the driver's existing machinery - LLVM IR via the clang
backend (--format llvm) or Polyglot IR via the native Rust driver
(--format pgir) - then annotates it: for every function, the IR header,
the source lines of that function as comments, then the IR body.  The
mapping is by function name (no debug info), so it is best-effort and
says so when a function cannot be located in the source file.
"""

import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from polyglot_lib import SCRIPT_DIR
from polyglot_lib.backends import common_compile_args, invoke_compiler
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, MissingToolError, PolyglotError, exit_status, log_running,
    run_cmd,
)

ORDER = 45


# --------------------------------------------------------------------------
# IR emission (existing machinery)
# --------------------------------------------------------------------------

def _emit_llvm_text(tc, src, args):
    """LLVM IR text for one TU via the clang backend (as 'emit-ir')."""
    compiler = tc.compiler_for([src])
    extra = common_compile_args(tc, args, [src], for_emit=True)
    with tempfile.TemporaryDirectory() as tmpdir:
        out = Path(tmpdir) / "module.ll"
        cmd = [compiler] + extra + ["-S", "-emit-llvm", str(src),
                                    "-o", str(out)]
        invoke_compiler(cmd, args, what="IR emission")
        return out.read_text(errors="replace")


def _find_native_driver():
    """The Rust polyglot-driver binary: PATH, then workspace target/release."""
    found = shutil.which("polyglot-driver")
    if found:
        return Path(found)
    for base in (SCRIPT_DIR / "target" / "release",
                 Path.cwd() / "target" / "release"):
        cand = base / "polyglot-driver"
        try:
            if cand.is_file() and os.access(cand, os.X_OK):
                return cand
        except OSError:
            continue
    return None


def _pgir_ignored_flags(args):
    """Common flags the native driver's emit-ir does not accept."""
    ignored = []
    if getattr(args, "std", None):
        ignored.append("--std")
    if getattr(args, "warn", "default") not in (None, "default"):
        ignored.append("--warn")
    if getattr(args, "werror", False):
        ignored.append("--werror")
    if getattr(args, "debug", False):
        ignored.append("--debug")
    if getattr(args, "sanitize", None):
        ignored.append("--sanitize")
    if getattr(args, "defines", None):
        ignored.append("-D")
    if getattr(args, "cc_args", None):
        ignored.append("--cc-arg")
    return ignored


def _emit_pgir_text(tc, src, args):
    """Polyglot IR text via the native Rust driver ('emit-ir --format pgir')."""
    driver = _find_native_driver()
    if driver is None:
        raise MissingToolError(
            "polyglot-driver", searched="PATH, target/release",
            purpose="--format pgir (build it with: cargo build --release "
                    "-p polyglot-driver)")
    ignored = _pgir_ignored_flags(args)
    if ignored:
        sys.stderr.write("warning: the native (pgir) backend ignores: %s\n"
                         % ", ".join(ignored))
    cmd = [str(driver), "emit-ir", "--format", "pgir", "-O",
           str(getattr(args, "opt_level", None) or "default")]
    target = getattr(args, "target", None)
    if target:
        cmd += ["--target", str(target)]
    for inc in getattr(args, "includes", None) or []:
        cmd += ["-I", str(inc)]
    cmd.append(str(src))
    log_running(cmd, args)
    proc = run_cmd(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        detail = (proc.stderr or "").strip()
        if detail:
            sys.stderr.write(detail + "\n")
        raise PolyglotError("pgir emission failed with exit code %d"
                            % proc.returncode,
                            EXIT_FAILURE if proc.returncode > 0
                            else exit_status(proc.returncode))
    return proc.stdout or ""


# --------------------------------------------------------------------------
# IR slicing: preamble / function blocks / epilogue
# --------------------------------------------------------------------------

_LLVM_DEFINE = re.compile(r'^define\b[^@]*@("?)([A-Za-z_.$][\w.$-]*)\1\s*\(')
_PGIR_DEFINE = re.compile(r"^func\s+@([\w.$-]+)\s*\(")


def _slice_ir(text, fmt):
    """Split IR text into (preamble, [(name, block_lines)], epilogue).

    A function block runs from its header line ('define ... @name(' or
    'func @name(' ending in '{') to the closing '}' at column zero.
    """
    def opener(ln):
        if fmt == "llvm":
            m = _LLVM_DEFINE.match(ln)
            return m.group(2) if m else None
        m = _PGIR_DEFINE.match(ln)
        if m and ln.rstrip().endswith("{"):
            return m.group(1)
        return None

    pre, post, funcs = [], [], []
    cur_name, cur = None, None
    for ln in text.splitlines():
        if cur is None:
            name = opener(ln)
            if name is not None:
                cur_name, cur = name, [ln]
            elif funcs:
                post.append(ln)
            else:
                pre.append(ln)
        else:
            cur.append(ln)
            if ln.rstrip() == "}":
                funcs.append((cur_name, cur))
                cur_name, cur = None, None
    if cur is not None:                # unterminated tail: keep it visible
        funcs.append((cur_name, cur))
    return pre, funcs, post


# --------------------------------------------------------------------------
# Source mapping (by function name; no debug info involved)
# --------------------------------------------------------------------------

def _brace_span(text, open_idx):
    """Index of the '}' matching the '{' at open_idx; -1 if unbalanced.
    Tolerates line/block comments and string/char literals."""
    depth = 0
    i = open_idx
    n = len(text)
    in_line = in_block = in_str = in_chr = False
    while i < n:
        c = text[i]
        nxt = text[i + 1] if i + 1 < n else ""
        if in_line:
            if c == "\n":
                in_line = False
        elif in_block:
            if c == "*" and nxt == "/":
                in_block = False
                i += 1
        elif in_str:
            if c == "\\":
                i += 1
            elif c == '"':
                in_str = False
        elif in_chr:
            if c == "\\":
                i += 1
            elif c == "'":
                in_chr = False
        else:
            if c == "/" and nxt == "/":
                in_line = True
                i += 1
            elif c == "/" and nxt == "*":
                in_block = True
                i += 1
            elif c == '"':
                in_str = True
            elif c == "'":
                in_chr = True
            elif c == "{":
                depth += 1
            elif c == "}":
                depth -= 1
                if depth == 0:
                    return i
        i += 1
    return -1


def _scan_to_open_brace(text, idx):
    """From just after the '(' that opened a candidate parameter list, find
    the '{' that opens the body.  None when a ';' (prototype or call
    statement) or an unbalanced ')' comes first."""
    depth = 1
    i = idx + 1
    limit = min(len(text), idx + 4096)
    while i < limit:
        c = text[i]
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth < 0:
                return None
        elif depth == 0:
            if c == "{":
                return i
            if c in ";}":
                return None
        i += 1
    return None


def _definition_span(text, name):
    """(name_offset, brace_idx, end_idx) of `name`'s definition, or None.

    Call sites and prototypes are skipped because no body brace follows
    their parameter list; the brace matcher tolerates comments/strings.
    """
    pat = re.compile(r"(?<![A-Za-z0-9_$])%s\s*\(" % re.escape(name))
    for m in pat.finditer(text):
        line_start = text.rfind("\n", 0, m.start()) + 1
        head = text[line_start:m.start()]
        if head.lstrip().startswith("#") or "//" in head:
            continue
        brace = _scan_to_open_brace(text, m.end() - 1)
        if brace is None:
            continue
        end = _brace_span(text, brace)
        if end < 0:
            end = len(text) - 1
        return m.start(), brace, end
    return None


def _line_of(text, offset):
    return text.count("\n", 0, offset) + 1


def _source_excerpt(src_text, name):
    """(start_line, end_line, [source lines]) for a definition, or None."""
    hit = _definition_span(src_text, name)
    if hit is None:
        return None
    name_off, _, end_idx = hit
    start = _line_of(src_text, name_off)
    end = _line_of(src_text, end_idx)
    lines = src_text.splitlines()
    return start, end, lines[start - 1:end]


def _demangle_base(tc, name):
    """Plain source identifier for an IR function name: C++ mangled names
    are run through llvm-cxxfilt when available; everything else passes
    through unchanged."""
    if not name.startswith("_Z"):
        return name
    filt = tc.find_tool("llvm-cxxfilt") if tc is not None else None
    if filt is None:
        return name
    try:
        proc = run_cmd([str(filt), name], capture_output=True, text=True,
                       timeout=10)
    except (OSError, subprocess.SubprocessError):
        return name
    if proc.returncode != 0:
        return name
    first = (proc.stdout or "").strip().splitlines()
    dem = first[0] if first else ""
    base = dem.split("(")[0].split("::")[-1].strip()
    if not re.match(r"^[A-Za-z_]\w*$", base):
        return name
    return base


# --------------------------------------------------------------------------
# Rendering
# --------------------------------------------------------------------------

_LLVM_LEGEND = [
    ("%x = alloca <type>", "reserve a stack slot (a local variable)"),
    ("%x = load <type>, ptr %p", "read the value a pointer points to"),
    ("store <type> %v, ptr %p", "write a value through a pointer"),
    ("%x = getelementptr ...", "address of an array/struct element"),
    ("%x = icmp <cond> %a, %b", "integer compare (eq, ne, slt, sgt, ...)"),
    ("br label %bb", "unconditional jump"),
    ("br i1 %c, label %t, label %f", "conditional jump"),
    ("%x = call <ty> @f(...)", "function call"),
    ("ret <ty> %v", "return from function"),
]

_PGIR_LEGEND = [
    ('slot s0: int "a"', "named local slot (parameter or variable)"),
    ("%r = const 42 : int", "constant into a register"),
    ("store.local s0, %r0", "write a local slot"),
    ("%r = load.local s0", "read a local slot"),
    ("%r = add.int %a, %b", "arithmetic (add sub mul div mod cmp.* ...)"),
    ("br bb1", "unconditional jump"),
    ("br.cond %c, bb1, bb2", "conditional jump"),
    ("%r = call @f(...)", "function call"),
    ("ret %r", "return from function"),
]


def _render_function(tc, name, block, src, src_text):
    """Header line, source excerpt as ';' comments, then the IR body."""
    lines = [block[0].rstrip()]
    lookup = _demangle_base(tc, name)
    found = _source_excerpt(src_text, lookup)
    if found is None and lookup != name:
        found = _source_excerpt(src_text, name)
    if found is not None:
        start, end, excerpt = found
        lines.append("; ---- source of '%s' (%s:%d-%d) ----"
                     % (lookup, src, start, end))
        for sl in excerpt:
            lines.append("; %s" % sl.rstrip())
    else:
        lines.append("; ---- source of '%s': not found in %s (defined "
                     "elsewhere, or renamed by the compiler) ----"
                     % (name, src))
    lines.extend(block[1:])
    return lines


@command("explain-ir")
def cmd_explain_ir(tc, args):
    src = Path(args.path)
    if not src.exists():
        raise PolyglotError("Source file not found: %s" % src)

    fmt = args.format
    if fmt == "llvm" and tc.kind != "clang":
        raise PolyglotError(
            "--format llvm requires the clang backend (current backend: %s); "
            "retry with: polyglot --compiler clang explain-ir %s ... or use "
            "--format pgir" % (tc.kind or "none", src))
    if fmt == "llvm":
        text = _emit_llvm_text(tc, src, args)
        dialect = "LLVM IR"
    else:
        text = _emit_pgir_text(tc, src, args)
        dialect = "Polyglot IR"

    try:
        src_text = src.read_text(errors="replace")
    except OSError as exc:
        raise PolyglotError("cannot read %s: %s" % (src, exc))

    pre, funcs, post = _slice_ir(text, fmt)
    wanted = list(args.functions or [])

    print("; explain-ir: %s" % src)
    print("; format: %s (%s)" % (fmt, dialect))
    print("; mapping: IR function names matched to source definitions "
          "(heuristic, no debug info); annotations are ';' comments")
    for ln in pre:
        print(ln)
    if not funcs:
        print("; note: no function definitions found in the IR")
    matched = set()
    for name, block in funcs:
        if wanted:
            base = _demangle_base(tc, name)
            hits = [w for w in wanted if w in (name, base)]
            if not hits:
                continue
            matched.update(hits)
        print()
        for ln in _render_function(tc, name, block, src, src_text):
            print(ln)
    for w in wanted:
        if w not in matched:
            sys.stderr.write("warning: function '%s' not found in the IR\n"
                             % w)
    if post:
        print()
        for ln in post:
            print(ln)
    print()
    print("Legend (common %s mnemonics):" % dialect)
    for mnemonic, meaning in (_LLVM_LEGEND if fmt == "llvm"
                              else _PGIR_LEGEND):
        print("  %-30s %s" % (mnemonic, meaning))


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "explain-ir",
        help="Show IR with the source of each function beside it")
    p.add_argument("path", metavar="FILE", help="source file to explain")
    p.add_argument("--format", choices=["pgir", "llvm"], default="llvm",
                   help="IR dialect: llvm = LLVM IR (clang backend), "
                        "pgir = Polyglot IR (native Rust driver) "
                        "(default: llvm)")
    p.add_argument("--functions", nargs="+", metavar="NAME", default=[],
                   help="only explain these functions (default: all)")
    global_flags.add_common_flags(p)
