"""polyglot.toml project-file loading: find a project directory from the
paths passed to build-like commands and resolve its sources/output, plus
multi-target ([[bin]] x N / [[lib]] x N) declaration parsing."""

from pathlib import Path

from polyglot_lib import toml
from polyglot_lib.output import EXIT_USAGE, PolyglotError, err_print

#: valid [[lib]] kinds; a lib without 'kind' defaults to static.
LIB_KINDS = ("static", "shared")

C_EXT_GLOBS = [".c", ".cpp", ".cc", ".cxx"]


def find_project_dir(paths):
    """Return (project_dir, config) when a polyglot.toml project was given."""
    if len(paths) == 1 and Path(paths[0]).is_dir():
        cfg = Path(paths[0]) / "polyglot.toml"
        if cfg.exists() and toml is not None:
            try:
                return Path(paths[0]), toml.load(cfg)
            except Exception as exc:
                err_print("warning: could not parse %s: %s" % (cfg, exc))
    return None, None


def _table_entries(config, key):
    """[[key]] entries as a list of tables; a lone [key] table is tolerated."""
    entries = config.get(key) or []
    if isinstance(entries, dict):
        entries = [entries]
    return [e for e in entries if isinstance(e, dict)]


def parse_bin_entries(config):
    """Every [[bin]] table, in declaration order."""
    return _table_entries(config, "bin")


def parse_lib_entries(config, project_dir):
    """[[lib]] tables validated and resolved to build targets.

    Each result: {"kind": "static"|"shared", "name": str,
    "sources": [Path]}.  Malformed declarations raise a friendly
    PolyglotError (exit 2).
    """
    targets = []
    for entry in _table_entries(config, "lib"):
        name = entry.get("name")
        if not name:
            raise PolyglotError("polyglot.toml [[lib]] needs a 'name' key",
                                EXIT_USAGE)
        kind = entry.get("kind", "static")
        if kind not in LIB_KINDS:
            raise PolyglotError(
                "polyglot.toml [[lib]] '%s': unknown kind '%s' (choose from: "
                "%s)" % (name, kind, ", ".join(LIB_KINDS)), EXIT_USAGE)
        sources = [project_dir / s for s in (entry.get("sources") or [])]
        if not sources:
            raise PolyglotError("polyglot.toml [[lib]] '%s' lists no sources"
                                % name, EXIT_USAGE)
        targets.append({"kind": kind, "name": name, "sources": sources})
    return targets


def _same_file(a, b):
    try:
        return a.resolve() == b.resolve()
    except OSError:
        return a == b


def glob_src_tus(project_dir):
    """Every translation unit under src/ (sorted per extension)."""
    sources = []
    for ext in C_EXT_GLOBS:
        sources.extend(sorted((project_dir / "src").glob("*" + ext)))
    return sources


def resolve_bin_sources(project_dir, declared, other_declared=()):
    """Sources for one [[bin]] target.

    Every translation unit under src/ is part of the build, except the
    declared sources of *other* [[bin]] targets (each binary must link
    exactly one main()).  The declared source joins at the front when it
    lives outside src/; with no other bins this matches the historical
    single-bin resolution exactly.
    """
    sources = [s for s in glob_src_tus(project_dir)
               if not any(_same_file(s, o) for o in other_declared)]
    if declared is not None:
        try:
            dup = declared.resolve() in {f.resolve() for f in sources}
        except OSError:
            dup = declared in sources
        if not dup:
            sources.insert(0, declared)
    return sources


def project_sources_and_output(project_dir, config, default_output):
    """Resolve [[bin]] / [language] config to (sources, output)."""
    bins = config.get("bin", [])
    output = default_output
    if bins and bins[0].get("name"):
        output = project_dir / bins[0]["name"]

    language = (config.get("language") or {})
    if language.get("rust"):
        src_dir = project_dir / "src"
        rust_src = src_dir / "main.rs"
        if rust_src.exists():
            return [rust_src], output
        return [], output
    # Every translation unit under src/ is part of the binary; skipping any
    # of them would fail the link with undefined references.
    sources = glob_src_tus(project_dir)
    if bins and "source" in bins[0]:
        primary = project_dir / bins[0]["source"]
        try:
            known = {f.resolve() for f in sources}
            dup = primary.resolve() in known
        except OSError:
            dup = primary in sources
        if not dup:
            sources.insert(0, primary)
    return sources, output
