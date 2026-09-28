"""The 'graph' command: extract and render the #include graph of source
files.

Each translation unit is preprocessed once with -H, which makes the
compiler print one '<dots> <path>' line per inclusion event on stderr
(clang and gcc use the same shape).  That tree carries the real include
structure - who includes whom, including system headers - which a flat
-MM listing cannot express.  A TU whose -H tree comes back empty is
re-scanned with the deps machinery (-MM, like 'polyglot deps') so its
flat dependencies still appear.

System headers (anything outside the current directory, the -I dirs and
the TU directories) render as optional gray nodes; --no-system hides
them.  Include cycles are reported as an error listing the cycle.
Renderers are deterministic: nodes and edges keep first-seen order, the
SVG layout is a pure function of that order.
"""

import json
import re
import sys
from collections import deque
from pathlib import Path

from polyglot_lib.backends import common_compile_args
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_USAGE, PolyglotError, run_cmd,
)

ORDER = 75

#: -H emits '<dots> <path>' per inclusion event ('. ./a.h', '.. ./b.h').
_INCLUDE_LINE = re.compile(r"^(\.+) (.+)$")

#: capacity of the hand-rolled SVG writer.
SVG_MAX_NODES = 200
_CYCLE_REPORT_MAX = 5

# box geometry for the SVG layout
_BOX_W, _BOX_H = 190, 36
_H_GAP, _V_GAP, _MARGIN = 28, 64, 24

# kind -> (fill, stroke, text color); system headers render gray.
_SVG_COLORS = {
    "tu": ("#3d6cb3", "#2c4f86", "#ffffff"),
    "project": ("#dde8f6", "#5b7fb5", "#1c2b3a"),
    "system": ("#d4d4d4", "#8f8f8f", "#333333"),
}


def _min_rotation(loop):
    """Canonical form of a cycle (a -> b -> a): smallest rotation of the
    node sequence without the closing repeat, so the same cycle reported
    from different starting points deduplicates."""
    seq = loop[:-1] if len(loop) > 1 and loop[0] == loop[-1] else list(loop)
    return min(tuple(seq[i:] + seq[:i]) for i in range(len(seq))) if seq \
        else tuple(loop)


class IncludeGraph:
    """Include DAG assembled from per-TU -H trees (or -MM fallbacks)."""

    def __init__(self, cwd, project_dirs):
        self.cwd = Path(cwd).resolve()
        self.project_dirs = [Path(d).resolve() for d in project_dirs]
        self.paths = {}        # abs path str -> {"name", "kind"}
        self.order = []        # abs path str, first-seen order
        self.edges = []        # (parent, child) abs paths, first-seen unique
        self._edge_set = set()
        self.cycles = []       # [abs, ..., abs] with the closing repeat
        self._cycle_seen = set()

    # -- nodes -------------------------------------------------------------

    def _display(self, abs_path):
        try:
            return str(Path(abs_path).relative_to(self.cwd))
        except ValueError:
            return str(abs_path)

    def _kind(self, abs_path):
        p = Path(abs_path)
        for base in self.project_dirs:
            try:
                p.relative_to(base)
                return "project"
            except ValueError:
                continue
        return "system"

    def add_node(self, abs_path, kind=None):
        """Register a node; first kind wins except 'tu' (a root)."""
        info = self.paths.get(abs_path)
        if info is None:
            info = {"name": self._display(abs_path),
                    "kind": kind or self._kind(abs_path)}
            self.paths[abs_path] = info
            self.order.append(abs_path)
        elif kind == "tu":
            info["kind"] = "tu"
        return abs_path

    def canon(self, raw):
        """Compiler-reported path -> canonical absolute path string."""
        p = Path(raw.strip())
        if not p.is_absolute():
            p = self.cwd / p
        return str(p.resolve())

    # -- edges ---------------------------------------------------------------

    def add_edge(self, parent, child):
        key = (parent, child)
        if key not in self._edge_set:
            self._edge_set.add(key)
            self.edges.append(key)

    def absorb_tree(self, root_abs, tree):
        """Fold one TU's (depth, path) inclusion tree into the graph."""
        self.add_node(root_abs, "tu")
        stack = [root_abs]                 # current include chain
        ancestors = {root_abs}
        for depth, raw in tree:
            child = self.add_node(self.canon(raw))
            while len(stack) > depth and len(stack) > 1:
                ancestors.discard(stack.pop())
            parent = stack[-1]
            self.add_edge(parent, child)
            if child in ancestors:
                # guard- or once-blocked re-include: a back-edge; nothing
                # can be nested below it in the tree
                self._record_cycle(stack, child)
                continue
            stack.append(child)
            ancestors.add(child)

    def _record_cycle(self, stack, child):
        try:
            idx = stack.index(child)
        except ValueError:
            idx = 0
        loop = stack[idx:] + [child]
        key = _min_rotation(loop)
        if key not in self._cycle_seen:
            self._cycle_seen.add(key)
            self.cycles.append(loop)

    def raise_if_cyclic(self):
        """Error out (exit 1) listing every distinct include cycle."""
        if not self.cycles:
            return
        parts = [" -> ".join(self.paths[p]["name"] for p in loop)
                 for loop in self.cycles[:_CYCLE_REPORT_MAX]]
        msg = "include cycle detected: %s" % "; ".join(parts)
        if len(self.cycles) > len(parts):
            msg += " (+%d more)" % (len(self.cycles) - len(parts))
        raise PolyglotError(msg, EXIT_FAILURE)


def project_dirs_for(args, files, cwd):
    """Everything that counts as 'project space' for the system/node
    split: the working directory, every -I dir and every TU's directory."""
    dirs = [Path(cwd)]
    for inc in getattr(args, "includes", None) or []:
        p = Path(str(inc))
        dirs.append(p if p.is_absolute() else Path(cwd) / p)
    for f in files:
        dirs.append(Path(f).resolve().parent)
    return dirs


def _scan_tree(tc, args, src):
    """One TU's inclusion tree as (depth, path) pairs, via the compiler."""
    compiler = tc.compiler_for([src])
    base = common_compile_args(tc, args, [src])
    proc = run_cmd([compiler] + base + ["-H", "-fsyntax-only", str(src)],
                   capture_output=True, text=True)
    if proc.returncode != 0:
        # echo the diagnostics without the -H tree noise
        diag = [l for l in (proc.stderr or "").splitlines()
                if not _INCLUDE_LINE.match(l)]
        if diag:
            sys.stderr.write("\n".join(diag) + "\n")
        raise PolyglotError("include scan failed for %s (exit code %d)"
                            % (src, proc.returncode),
                            proc.returncode or EXIT_FAILURE)
    tree = []
    for line in (proc.stderr or "").splitlines():
        m = _INCLUDE_LINE.match(line)
        if m:
            tree.append((len(m.group(1)), m.group(2).strip()))
    if not tree:
        tree = _deps_fallback(tc, compiler, base, src)
    return tree


def _deps_fallback(tc, compiler, base, src):
    """-MM fallback (deps machinery) for TUs whose -H tree came back
    empty: the flat Make dependency list becomes depth-1 edges."""
    proc = run_cmd([compiler] + base + ["-MM", str(src)],
                   capture_output=True, text=True)
    if proc.returncode != 0:
        return []
    text = (proc.stdout or "").replace("\\\n", " ")
    _target, sep, rest = text.partition(":")
    if not sep:
        return []
    src_resolved = Path(src).resolve()
    return [(1, dep) for dep in rest.split()
            if Path(dep).resolve() != src_resolved]


def collect_graph(tc, args, files, cwd):
    """Scan every TU and merge the trees into one IncludeGraph."""
    g = IncludeGraph(cwd, project_dirs_for(args, files, cwd))
    roots = []
    for f in files:
        rp = str(Path(f).resolve())
        if rp not in roots:
            roots.append(rp)
    for rp in roots:
        g.add_node(rp, "tu")
    for rp in roots:
        g.absorb_tree(rp, _scan_tree(tc, args, Path(rp)))
    g.raise_if_cyclic()
    return g, roots


def visible_graph(g, hide_system):
    """Nodes/edges after the --no-system filter, as (abs path list, edge
    list) in deterministic first-seen order."""
    nodes = [p for p in g.order
             if not (hide_system and g.paths[p]["kind"] == "system")]
    keep = set(nodes)
    edges = [(a, b) for (a, b) in g.edges if a in keep and b in keep]
    return nodes, edges


# --------------------------------------------------------------------------
# Renderers
# --------------------------------------------------------------------------

def render_ascii(g, roots, nodes, edges):
    """Indented include tree; repeats are marked '(seen)' and a defensive
    '(cycle)' marker keeps pathological graphs from recursing forever."""
    children = {}
    for parent, child in edges:
        children.setdefault(parent, []).append(child)
    rendered = set()
    lines = []

    def walk(node, depth, on_path):
        info = g.paths[node]
        marks = []
        if info["kind"] == "system":
            marks.append("system")
        if node in on_path:
            marks.append("cycle")
        elif node in rendered:
            marks.append("seen")
        lines.append("%s- %s" % ("  " * depth,
                    info["name"] + (" [%s]" % ", ".join(marks) if marks
                                    else "")))
        if node in on_path or node in rendered:
            return
        rendered.add(node)
        for child in children.get(node, []):
            walk(child, depth + 1, on_path | {node})

    for i, root in enumerate(roots):
        if root not in g.paths:
            continue
        if i:
            lines.append("")
        lines.append(g.paths[root]["name"])
        for child in children.get(root, []):
            walk(child, 1, {root})
    return "\n".join(lines)


def render_json(g, nodes, edges):
    return json.dumps({
        "nodes": [{"name": g.paths[p]["name"], "kind": g.paths[p]["kind"]}
                  for p in nodes],
        "edges": [{"from": g.paths[a]["name"], "to": g.paths[b]["name"]}
                  for (a, b) in edges],
    }, indent=2)


def _svg_escape(text):
    return (str(text).replace("&", "&amp;").replace("<", "&lt;")
            .replace(">", "&gt;").replace('"', "&quot;"))


def _svg_label(name):
    return _svg_escape(name if len(name) <= 26 else name[:23] + "...")


def _layers(nodes, edges):
    """Longest-path layering over the DAG (cycles were already fatal)."""
    indeg = {p: 0 for p in nodes}
    children = {p: [] for p in nodes}
    for parent, child in edges:
        children[parent].append(child)
        indeg[child] += 1
    layer = {p: 0 for p in nodes}
    queue = deque(p for p in nodes if indeg[p] == 0)
    while queue:
        n = queue.popleft()
        for c in children[n]:
            layer[c] = max(layer[c], layer[n] + 1)
            indeg[c] -= 1
            if indeg[c] == 0:
                queue.append(c)
    return layer


def render_svg(g, nodes, edges):
    """Hand-rolled dependency-free SVG: boxes per header, arrows per
    include edge, rows layered by depth.  Deterministic output."""
    if len(nodes) > SVG_MAX_NODES:
        raise PolyglotError(
            "include graph has %d nodes; SVG rendering supports up to %d "
            "(try --no-system, or --format json/ascii)"
            % (len(nodes), SVG_MAX_NODES), EXIT_FAILURE)

    layer = _layers(nodes, edges)
    per_layer = {}
    for p in nodes:                       # keeps first-seen order within rows
        per_layer.setdefault(layer[p], []).append(p)
    pos = {}
    for ly in sorted(per_layer):
        for col, p in enumerate(per_layer[ly]):
            pos[p] = (_MARGIN + col * (_BOX_W + _H_GAP),
                      _MARGIN + ly * (_BOX_H + _V_GAP))
    width = _MARGIN * 2 + max(len(v) for v in per_layer.values()) \
        * (_BOX_W + _H_GAP) - _H_GAP
    height = _MARGIN * 2 + (max(layer.values()) + 1) * (_BOX_H + _V_GAP) \
        - _V_GAP

    out = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        '<svg xmlns="http://www.w3.org/2000/svg" width="%d" height="%d" '
        'viewBox="0 0 %d %d">' % (width, height, width, height),
        "<defs>",
        '<marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" '
        'markerWidth="7" markerHeight="7" orient="auto-start-reverse">',
        '<path d="M 0 0 L 10 5 L 0 10 z" fill="%s"/>' % _SVG_COLORS["project"][1],
        "</marker>",
        "</defs>",
        '<rect x="0" y="0" width="%d" height="%d" fill="#ffffff"/>'
        % (width, height),
    ]
    for parent, child in edges:
        x1, y1 = pos[parent][0] + _BOX_W // 2, pos[parent][1] + _BOX_H
        x2, y2 = pos[child][0] + _BOX_W // 2, pos[child][1]
        out.append('<line x1="%d" y1="%d" x2="%d" y2="%d" stroke="%s" '
                   'stroke-width="1.5" marker-end="url(#arrow)"/>'
                   % (x1, y1, x2, y2, _SVG_COLORS["project"][1]))
    for p in nodes:
        fill, stroke, text_fill = _SVG_COLORS[g.paths[p]["kind"]]
        x, y = pos[p]
        out.append(
            '<g><rect x="%d" y="%d" width="%d" height="%d" rx="6" '
            'fill="%s" stroke="%s"/><text x="%d" y="%d" '
            'text-anchor="middle" font-family="monospace" font-size="12" '
            'fill="%s">%s</text></g>'
            % (x, y, _BOX_W, _BOX_H, fill, stroke,
               x + _BOX_W // 2, y + _BOX_H // 2 + 4, text_fill,
               _svg_label(g.paths[p]["name"])))
    out.append("</svg>")
    return "\n".join(out) + "\n"


@command("graph")
def cmd_graph(tc, args):
    files = [Path(f) for f in args.files]
    for f in files:
        if not f.exists():
            raise PolyglotError("Source file not found: %s" % f)
        if f.is_dir():
            raise PolyglotError("Expected source file, got directory: %s" % f)
    cwd = Path.cwd()

    g, roots = collect_graph(tc, args, files, cwd)
    nodes, edges = visible_graph(g, bool(getattr(args, "no_system", False)))

    if args.format == "svg":
        text = render_svg(g, nodes, edges)
    elif args.format == "json":
        text = render_json(g, nodes, edges)
    else:
        text = render_ascii(g, roots, nodes, edges)

    if args.out:
        try:
            Path(args.out).write_text(text)
        except OSError as exc:
            raise PolyglotError("cannot write %s: %s" % (args.out, exc))
        print("Wrote %s graph to %s (%d nodes, %d edges)"
              % (args.format, args.out, len(nodes), len(edges)))
    else:
        sys.stdout.write(text if text.endswith("\n") or not text
                         else text + "\n")


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "graph", help="Render the #include graph of source files",
        description="Extract the #include graph of the given source files "
                    "(one compiler -H pass per TU, -MM fallback) and render "
                    "it as an ascii tree, json ({nodes, edges}) or a "
                    "dependency-free svg; system headers show as gray nodes "
                    "unless --no-system, include cycles are errors.")
    p.add_argument("files", nargs="+", help="source files to graph")
    p.add_argument("--format", choices=["svg", "ascii", "json"],
                   default="ascii", help="output format (default: ascii)")
    p.add_argument("--out", metavar="FILE",
                   help="write the graph here instead of stdout")
    p.add_argument("--no-system", dest="no_system", action="store_true",
                   help="hide system headers (shown as gray nodes by default)")
    global_flags.add_common_flags(p)
