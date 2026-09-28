"""The 'plugin' command: list, install, remove and inspect JSON-RPC plugins.

A plugin is an executable speaking the protocol implemented by
crates/polyglot-protocol (newline-delimited JSON-RPC 2.0 on stdio) plus a
polyglot-plugin.toml manifest declaring its name, version and capabilities.
'plugin install' copies a built binary and its manifest into the plugin
directory (<plugin-dir>/<name>/, by default ~/.polyglot/plugins/<name>/);
'plugin list' shows what is installed, built in this workspace, and
available from the registry index.
"""

import json
import os
import shutil
import stat
import subprocess
import tempfile
import urllib.request
from pathlib import Path

from polyglot_lib import SCRIPT_DIR, VERSION
from polyglot_lib.commands import command
from polyglot_lib.output import (
    EXIT_FAILURE, EXIT_USAGE, MissingToolError, PolyglotError, green, yellow,
)
from polyglot_lib.plugin_client import resolve_plugin

try:
    import toml
except ImportError:  # pragma: no cover - toml is only needed for manifests
    toml = None

ORDER = 85

MANIFEST_NAME = "polyglot-plugin.toml"

#: Fallback registry index when no local registry/index.json is found.
DEFAULT_REGISTRY_URL = ("https://raw.githubusercontent.com/RXVEN-1907/"
                        "polyglot-compiler/master/registry/index.json")

ACTIONS = ("list", "install", "remove", "info")


# --------------------------------------------------------------------------
# Manifest helpers
# --------------------------------------------------------------------------

def _strip_prefix(name):
    return name[len("polyglot-"):] if name.startswith("polyglot-") else name


def _load_manifest(path):
    """Parse a polyglot-plugin.toml; returns a dict or None."""
    if toml is None or path is None:
        return None
    try:
        data = toml.load(str(path))
    except Exception:
        return None
    return data if isinstance(data, dict) else None


def manifest_for(path):
    """Locate the manifest belonging to a plugin binary or plugin directory.

    Checked in order: the directory itself, a manifest sitting next to the
    binary, and the workspace source tree (plugins/<name>/polyglot-plugin.toml,
    where the built-in plugins live).
    """
    p = Path(path)
    if p.is_dir():
        cand = p / MANIFEST_NAME
        return cand if cand.is_file() else None
    cand = p.parent / MANIFEST_NAME
    if cand.is_file():
        return cand
    for base in (SCRIPT_DIR, Path.cwd()):
        for stem in (p.name, _strip_prefix(p.name)):
            cand = base / "plugins" / stem / MANIFEST_NAME
            if cand.is_file():
                return cand
    return None


def manifest_version(path):
    """The manifest's [plugin] version, or '?' when unreadable."""
    data = _load_manifest(manifest_for(path))
    version = (data.get("plugin") or {}).get("version") if data else None
    return str(version) if version else "?"


def _manifest_value(data, table, key):
    entry = data.get(table) or {} if data else {}
    value = entry.get(key) if isinstance(entry, dict) else None
    return str(value) if value not in (None, "") else None


# --------------------------------------------------------------------------
# Registry index
# --------------------------------------------------------------------------

def _fetch_json(url, timeout=15.0):
    req = urllib.request.Request(
        url, headers={"User-Agent": "polyglot/%s" % VERSION})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def _read_index_source(source):
    """Read a registry index from a URL (https JSON) or a local file path."""
    if "://" in source:
        try:
            return _fetch_json(source), source
        except Exception as exc:
            raise PolyglotError("cannot fetch registry index %s: %s"
                                % (source, exc))
    path = Path(source).expanduser()
    if not path.is_file():
        raise PolyglotError("registry index file not found: %s" % source,
                            EXIT_USAGE)
    try:
        return json.loads(path.read_text(encoding="utf-8")), str(path)
    except (OSError, ValueError) as exc:
        raise PolyglotError("cannot read registry index %s: %s" % (source, exc))


def load_index(registry=None, soft=False):
    """Load the registry index; returns (entries, source) or (None, source).

    Precedence: explicit --registry, then registry/index.json in the working
    directory or the workspace, then the default remote index.  With soft=True
    an unavailable default remote degrades to (None, None) instead of failing.
    """
    if registry:
        data, source = _read_index_source(registry)
        return _index_entries(data), source
    for cand in (Path.cwd() / "registry" / "index.json",
                 SCRIPT_DIR / "registry" / "index.json"):
        if cand.is_file():
            try:
                return _index_entries(json.loads(cand.read_text(
                    encoding="utf-8"))), str(cand)
            except (OSError, ValueError):
                continue
    try:
        return _index_entries(_fetch_json(DEFAULT_REGISTRY_URL)), \
            DEFAULT_REGISTRY_URL
    except Exception as exc:
        if soft:
            return None, None
        raise PolyglotError(
            "no registry index found (looked in ./registry, the workspace, "
            "and %s, which failed: %s)\nHint: pass --registry with an index "
            "URL or file" % (DEFAULT_REGISTRY_URL, exc))


def _index_entries(data):
    """Normalize the index JSON into dicts with name/binary/version/... keys."""
    plugins = data.get("plugins") if isinstance(data, dict) else None
    if not isinstance(plugins, list):
        return []
    entries = []
    for raw in plugins:
        if not isinstance(raw, dict):
            continue
        name = raw.get("name")
        if not name:
            continue
        entries.append({
            "name": str(name),
            "binary": str(raw.get("plugin") or raw.get("binary")
                          or ("polyglot-" + str(name))),
            "version": str(raw.get("version") or "?"),
            "description": str(raw.get("description") or ""),
            "repository": str(raw.get("repository") or raw.get("repo")
                              or raw.get("url") or ""),
            "path": str(raw.get("path") or ""),
        })
    return entries


def registry_lookup(name, registry=None):
    """Find one index entry by registry name or binary name, or None."""
    entries, source = load_index(registry)
    for entry in entries or []:
        if name in (entry["name"], entry["binary"]):
            return entry, source
    return None, source


# --------------------------------------------------------------------------
# plugin list
# --------------------------------------------------------------------------

def _executable(path):
    try:
        return path.is_file() and os.access(path, os.X_OK)
    except OSError:
        return False


def _builtin_binaries():
    """polyglot-* executables built in this workspace (target/release)."""
    found = {}
    for base in (SCRIPT_DIR / "target" / "release",
                 Path.cwd() / "target" / "release"):
        try:
            entries = sorted(base.iterdir())
        except OSError:
            continue
        for f in entries:
            if f.name == "polyglot-driver" or not _executable(f):
                continue
            if f.name.startswith("polyglot-"):
                found.setdefault(f.name, f)
    return [found[k] for k in sorted(found)]


def _installed_binary(d):
    """The executable inside an installed plugin directory, or None."""
    names = [d.name, _strip_prefix(d.name)]
    command = _manifest_value(_load_manifest(d / MANIFEST_NAME),
                              "build", "command")
    if command:
        names.append(command)
    for n in dict.fromkeys(names):
        for cand in (d / n, d / (n + ".exe")):
            if _executable(cand):
                return cand
    try:
        for f in sorted(d.iterdir()):
            if _executable(f):
                return f
    except OSError:
        pass
    return None


def _collect_rows(plugin_dir, registry=None):
    """Rows of (name, version, kind, path), installed before builtin before
    registry, deduped by name so local shadows of registry entries win."""
    rows, seen = [], set()

    def add(name, version, kind, path, aliases=()):
        if name in seen or any(a in seen for a in aliases):
            return
        seen.add(name)
        seen.update(aliases)
        rows.append((name, version, kind, str(path)))

    root = Path(plugin_dir).expanduser()
    try:
        entries = sorted(root.iterdir())
    except OSError:
        entries = []
    for f in entries:
        if f.is_dir():
            binary = _installed_binary(f)
            if binary is not None:
                add(f.name, manifest_version(binary), "installed", binary)
        elif _executable(f):
            add(f.name, manifest_version(f), "installed", f)
    for binary in _builtin_binaries():
        add(binary.name, manifest_version(binary), "builtin", binary)
    index_entries, _source = load_index(registry, soft=True)
    if index_entries is None:
        print(yellow("note: registry index unavailable; listing local "
                     "plugins only"))
    for entry in index_entries or []:
        where = entry["repository"] or "(no repository)"
        if entry["path"]:
            where += " (%s)" % entry["path"]
        add(entry["name"], entry["version"], "registry", where,
            aliases=(entry["binary"],))
    return rows


def plugin_list(args):
    rows = _collect_rows(getattr(args, "plugin_dir", None)
                         or "~/.polyglot/plugins",
                         registry=getattr(args, "registry", None))
    if not rows:
        print("No plugins found.")
        print("Install one with: polyglot plugin install <path|git-url|name>")
        return
    widths = [max(len(r[i]) for r in rows)
              for i in range(3)] + [max(len(r[3]) for r in rows)]
    widths[0] = max(widths[0], len("NAME"))
    widths[1] = max(widths[1], len("VERSION"))
    widths[2] = max(widths[2], len("KIND"))
    print("%-*s  %-*s  %-*s  %s" % (widths[0], "NAME", widths[1], "VERSION",
                                    widths[2], "KIND", "PATH"))
    for name, version, kind, path in rows:
        print("%-*s  %-*s  %-*s  %s" % (widths[0], name, widths[1], version,
                                        widths[2], kind, path))


# --------------------------------------------------------------------------
# plugin install
# --------------------------------------------------------------------------

def _shallow_clone(url, dest):
    git = shutil.which("git")
    if git is None:
        raise MissingToolError("git", purpose="installing plugin from %s" % url)
    result = subprocess.run([git, "clone", "--depth", "1", str(url), str(dest)],
                            capture_output=True, text=True)
    if result.returncode != 0:
        raise PolyglotError("git clone %s failed:\n%s"
                            % (url, (result.stderr or result.stdout).strip()))
    return dest


def _find_package_dir(clone):
    """Where in a clone the plugin crate lives: a subdir given by the index,
    a root manifest, or the shallowest manifest found in the tree."""
    if (clone / MANIFEST_NAME).is_file():
        return clone
    try:
        matches = sorted(clone.rglob(MANIFEST_NAME),
                         key=lambda p: (len(p.parts), str(p)))
    except OSError:
        matches = []
    for m in matches:
        if "target" not in m.parts:
            return m.parent
    if matches:
        return matches[0].parent
    raise PolyglotError("no %s found in the cloned repository" % MANIFEST_NAME)


def _build_plugin(clone, package_dir, want=None):
    """cargo build --release in the cloned plugin crate; returns the binary."""
    cargo = shutil.which("cargo")
    if cargo is None:
        raise MissingToolError("cargo",
                               purpose="building the plugin from source")
    result = subprocess.run([cargo, "build", "--release"], cwd=str(package_dir),
                            capture_output=True, text=True)
    if result.returncode != 0:
        raise PolyglotError("cargo build --release failed:\n%s"
                            % (result.stderr or result.stdout).strip()[-2000:])

    command = _manifest_value(_load_manifest(package_dir / MANIFEST_NAME),
                              "build", "command")
    release_dirs = [clone / "target" / "release",
                    package_dir / "target" / "release"]
    for release in release_dirs:
        for name in dict.fromkeys(filter(None, [command, want])):
            for cand in (release / name, release / (name + ".exe")):
                if _executable(cand):
                    return cand
    for release in release_dirs:
        try:
            cands = [f for f in sorted(release.iterdir())
                     if _executable(f) and f.name.startswith("polyglot-")
                     and f.name != "polyglot-driver"]
        except OSError:
            continue
        if len(cands) == 1:
            return cands[0]
        if len(cands) > 1:
            raise PolyglotError(
                "several plugin binaries were built (%s); install one by "
                "explicit path instead" % ", ".join(c.name for c in cands))
    raise PolyglotError("no plugin binary found after cargo build "
                        "(looked in %s)" % ", ".join(str(d) for d in release_dirs))


def _install_dest(plugin_root, binary):
    """Copy a binary into plugin_root/<name>/, executable; returns its path."""
    dest_dir = plugin_root / binary.name
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest_bin = dest_dir / binary.name
    shutil.copy2(binary, dest_bin)
    os.chmod(dest_bin, os.stat(dest_bin).st_mode | stat.S_IXUSR
             | stat.S_IXGRP | stat.S_IXOTH)
    return dest_bin


def _announce_install(dest_bin, have_manifest):
    print(green("Installed %s -> %s" % (dest_bin.name, dest_bin)))
    if not have_manifest:
        print(yellow("note: no %s found; installed the binary only"
                     % MANIFEST_NAME))
    print("Use it with: polyglot --plugin %s build <sources>"
          % dest_bin.name)


def _install_from_git(url, subdir, plugin_root, want=None):
    """Clone (shallow), build, and install into plugin_root.

    The clone lives in a temporary directory, so the copy happens inside
    its context; returns the installed binary path.
    """
    with tempfile.TemporaryDirectory(prefix="polyglot-plugin-") as tmp:
        clone = _shallow_clone(url, Path(tmp) / "src")
        package_dir = clone / subdir if subdir else _find_package_dir(clone)
        if not (package_dir / MANIFEST_NAME).is_file():
            raise PolyglotError("no %s in %s (subdir %r)"
                                % (MANIFEST_NAME, url, subdir or "."))
        binary = _build_plugin(clone, package_dir, want=want)
        dest_bin = _install_dest(plugin_root, binary)
        _announce_install(dest_bin,
                          _copy_manifest(package_dir / MANIFEST_NAME,
                                         dest_bin.parent))
        return dest_bin


def _copy_manifest(manifest_src, dest_dir):
    """Copy a manifest next to the installed binary; True when copied."""
    if manifest_src is None or not manifest_src.is_file():
        return False
    shutil.copy2(manifest_src, dest_dir / MANIFEST_NAME)
    return True


def plugin_install(args):
    src = (args.name or "").strip()
    if not src:
        raise PolyglotError("usage: polyglot plugin install <path|git-url|"
                            "registry-name>", EXIT_USAGE)
    plugin_root = Path(args.plugin_dir).expanduser()

    if "/" in src or "\\" in src or Path(src).exists():
        p = Path(src).expanduser()
        if not p.is_file():
            raise PolyglotError("no such plugin binary: %s" % src, EXIT_FAILURE)
        if not os.access(p, os.X_OK):
            raise PolyglotError("not executable: %s" % src, EXIT_FAILURE)
        binary = p.resolve()
        dest_bin = _install_dest(plugin_root, binary)
        _announce_install(dest_bin,
                          _copy_manifest(manifest_for(binary),
                                         dest_bin.parent))
    elif "://" in src or src.startswith("git@"):
        _install_from_git(src, None, plugin_root)
    else:
        entry, source = registry_lookup(src, getattr(args, "registry", None))
        if entry is None:
            raise PolyglotError(
                "plugin '%s' not found in the registry index\nHint: pass "
                "--registry with an index URL or file" % src)
        if not entry["repository"]:
            raise PolyglotError("registry entry '%s' has no repository URL"
                                % entry["name"])
        print("Fetching %s v%s from %s"
              % (entry["name"], entry["version"], entry["repository"]))
        _install_from_git(entry["repository"], entry["path"] or None,
                          plugin_root, want=entry["binary"])


# --------------------------------------------------------------------------
# plugin remove / plugin info
# --------------------------------------------------------------------------

def _installed_paths(name, plugin_root):
    """Everything installed under plugin_root for NAME (dir or loose files)."""
    hits = []
    for n in dict.fromkeys([name, "polyglot-" + name]):
        d = plugin_root / n
        if d.is_dir():
            hits.append(d)
        for cand in (plugin_root / n, plugin_root / (n + ".exe")):
            if cand.is_file():
                hits.append(cand)
    return hits


def plugin_remove(args):
    name = (args.name or "").strip()
    if not name:
        raise PolyglotError("usage: polyglot plugin remove <name>", EXIT_USAGE)
    if "/" in name or "\\" in name:
        raise PolyglotError("plugin remove takes a name, not a path",
                            EXIT_USAGE)
    plugin_root = Path(args.plugin_dir).expanduser()
    hits = _installed_paths(name, plugin_root)
    if not hits:
        raise PolyglotError("plugin '%s' is not installed (looked in %s)"
                            % (name, plugin_root))
    for path in hits:
        if path.is_dir():
            shutil.rmtree(path)
        else:
            path.unlink()
        print("Removed %s" % path)


def _fmt_list(values):
    return ", ".join(str(v) for v in values) if values else "(none)"


def plugin_info(args):
    name = (args.name or "").strip()
    if not name:
        raise PolyglotError("usage: polyglot plugin info <name>", EXIT_USAGE)
    plugin_dir = getattr(args, "plugin_dir", None) or "~/.polyglot/plugins"
    binary = resolve_plugin(name, plugin_dir)
    manifest = manifest_for(binary) if binary is not None else None
    if manifest is None:
        manifest = Path(plugin_dir).expanduser() / name / MANIFEST_NAME
        if not manifest.is_file():
            manifest = None

    def out(key, value):
        print("%-16s%s" % (key, value))

    if binary is None and manifest is None:
        entry, _ = registry_lookup(name, getattr(args, "registry", None))
        if entry is None:
            raise PolyglotError("plugin '%s' not found (not installed, not "
                                "built in this workspace, not in the "
                                "registry)" % name, EXIT_FAILURE)
        out("name", entry["name"])
        out("version", entry["version"])
        out("kind", "registry")
        out("repository", entry["repository"] or "(none)")
        out("description", entry["description"] or "(none)")
        return

    data = _load_manifest(manifest) or {}
    out("name", name)
    out("binary", str(binary) if binary is not None else "(not found)")
    out("version", _manifest_value(data, "plugin", "version") or "?")
    out("description", _manifest_value(data, "plugin", "description")
        or "(none)")
    out("author", _manifest_value(data, "plugin", "author") or "(none)")
    out("license", _manifest_value(data, "plugin", "license") or "(none)")
    out("protocol", "%s (%s)" % (
        _manifest_value(data, "protocol", "version") or "?",
        _manifest_value(data, "protocol", "transport") or "?"))
    caps = data.get("capabilities") or {}
    out("languages", _fmt_list(caps.get("languages")))
    out("source exts", _fmt_list(caps.get("source_extensions")))
    out("mlir dialects", _fmt_list(caps.get("mlir_dialects")))
    out("build command", _manifest_value(data, "build", "command") or "(none)")
    if manifest is None:
        print(yellow("note: no %s found; capabilities unknown" % MANIFEST_NAME))


# --------------------------------------------------------------------------
# Registration
# --------------------------------------------------------------------------

@command("plugin", require=False)
def cmd_plugin(tc, args):
    action = args.action
    if action == "list":
        plugin_list(args)
    elif action == "install":
        plugin_install(args)
    elif action == "remove":
        plugin_remove(args)
    elif action == "info":
        plugin_info(args)


def register(subparsers, global_flags):
    p = subparsers.add_parser(
        "plugin", help="Manage JSON-RPC plugins (list/install/remove/info)")
    p.add_argument("action", choices=list(ACTIONS),
                   help="list: show installed, builtin and registry "
                        "plugins; install: install from a binary path, git "
                        "URL or registry name; remove: uninstall an "
                        "installed plugin; info: show a plugin's declared "
                        "capabilities")
    p.add_argument("name", nargs="?",
                   help="install: path, git URL or registry name; "
                        "remove/info: plugin name")
    p.add_argument("--registry", metavar="URL|PATH",
                   help="registry index URL or JSON file (overrides the "
                        "built-in registry locations)")
