"""Incremental build cache: a content-addressed object cache stored in
<project>/.polyglot-cache/."""

import hashlib
import json
import os
import subprocess
import threading
import time
from pathlib import Path

from polyglot_lib.output import run_cmd

# Manifest keys that hold persistent lookups counters rather than cache
# entries.  They live in the same JSON object for simplicity; entry keys are
# always 64-hex sha256 digests, so the two namespaces never collide.
COUNTER_KEYS = ("hits", "misses")


class BuildCache:
    """Content-addressed object cache stored in <project>/.polyglot-cache/."""

    def __init__(self, project_dir, enabled=True):
        self.enabled = enabled
        self.dir = Path(project_dir) / ".polyglot-cache"
        self.manifest_path = self.dir / "manifest.json"
        self._manifest = None

    def _load_manifest(self):
        if self._manifest is None:
            try:
                self._manifest = json.loads(self.manifest_path.read_text())
            except (OSError, ValueError):
                self._manifest = {}
        return self._manifest

    def _flush(self):
        """Atomically persist the in-memory manifest (best-effort).

        Uses a pid+thread-unique scratch name so concurrent writers never
        observe a half-written file, like store() has always done.
        """
        try:
            tmp_m = self.dir / ("manifest.%d.%d.json.tmp"
                                % (os.getpid(), threading.get_ident()))
            tmp_m.write_text(json.dumps(self._manifest, indent=1,
                                        sort_keys=True))
            os.replace(tmp_m, self.manifest_path)
        except (OSError, ValueError, TypeError):
            pass  # the cache is best-effort; counter loss must not fail builds

    @staticmethod
    def _counter(manifest, name):
        value = manifest.get(name, 0)
        return value if isinstance(value, int) and not isinstance(value, bool) \
            else 0

    @staticmethod
    def key(tc, flags, source, project_dir):
        """sha256 of (compiler path+version, flags, preprocessed TU, includes).

        The preprocessed translation unit is hashed (ccache's preprocessor
        mode) so a change to *any* included header invalidates the entry;
        old entries simply miss.  When preprocessing fails, the raw source
        bytes are hashed as a fallback.
        """
        import hashlib
        src = Path(source)
        content = None
        try:
            proc = run_cmd([str(tc.compiler_for([src]))]
                           + [str(f) for f in flags]
                           + ["-E", str(Path(source).resolve())],
                           capture_output=True, text=True, timeout=120)
            if proc.returncode == 0 and proc.stdout:
                content = proc.stdout.encode("utf-8", "surrogateescape")
        except (OSError, subprocess.SubprocessError):
            content = None
        if content is None:
            content = src.read_bytes()
        h = hashlib.sha256()
        payload = {
            "compiler": str(tc.cc_path),
            "version": tc.version,
            "flags": flags,
            "source": str(Path(source).resolve()),
            "bytes": hashlib.sha256(content).hexdigest(),
            "project": str(Path(project_dir).resolve()),
        }
        h.update(json.dumps(payload, sort_keys=True).encode())
        return h.hexdigest()

    def object_path(self, key):
        return self.dir / (key + ".o")

    def get(self, key):
        if not self.enabled:
            return None
        obj = self.object_path(key)
        manifest = self._load_manifest()
        if obj.is_file() and key in manifest:
            manifest["hits"] = self._counter(manifest, "hits") + 1
            self._flush()
            return obj
        manifest["misses"] = self._counter(manifest, "misses") + 1
        self._flush()
        return None

    def store(self, key, source, flags, tmp_obj):
        """Move a freshly compiled object into the cache (atomic rename)."""
        if not self.enabled:
            return
        try:
            self.dir.mkdir(parents=True, exist_ok=True)
            obj = self.object_path(key)
            os.replace(tmp_obj, obj)   # same dir -> atomic on POSIX
            entry = {
                "source": str(source),
                "flags": flags,
                "created": time.strftime("%Y-%m-%dT%H:%M:%S"),
            }
            try:
                # object content digest, so 'cache verify' can detect rot
                # in entries stored by this and later versions
                entry["sha256"] = hashlib.sha256(obj.read_bytes()).hexdigest()
            except OSError:
                pass
            manifest = self._load_manifest()
            manifest[key] = entry
            self._flush()
        except OSError:
            pass  # cache is best-effort; a failed store must not fail builds

    def clear(self):
        removed = 0
        if self.dir.is_dir():
            for f in self.dir.iterdir():
                try:
                    if f.is_file():
                        f.unlink()
                        removed += 1
                except OSError:
                    pass
        return removed

    # ------------------------------------------------------------------
    # Inspection and maintenance helpers (used by the 'cache' command)
    # ------------------------------------------------------------------

    def objects(self):
        """Sorted list of every *.o file currently in the cache directory."""
        if not self.dir.is_dir():
            return []
        try:
            return sorted(self.dir.glob("*.o"))
        except OSError:
            return []

    def entries(self):
        """Usable cache entries, one dict per manifest record whose object
        file still exists (that is exactly what get() would serve):
        {key, source, flags, created, size, mtime}."""
        manifest = self._load_manifest()
        out = []
        for key, rec in manifest.items():
            if key in COUNTER_KEYS or not isinstance(rec, dict):
                continue
            try:
                st = self.object_path(key).stat()
            except OSError:
                continue
            out.append({
                "key": key,
                "source": rec.get("source"),
                "flags": rec.get("flags", []),
                "created": rec.get("created"),
                "size": st.st_size,
                "mtime": st.st_mtime,
            })
        return out

    def orphans(self):
        """Object files on disk with no manifest record (never served)."""
        manifest = self._load_manifest()
        return [f for f in self.objects()
                if not isinstance(manifest.get(f.stem), dict)]

    def stats(self):
        """Summary snapshot for the 'cache stats' report."""
        ents = self.entries()
        orphs = self.orphans()
        orph_bytes = 0
        for f in orphs:
            try:
                orph_bytes += f.stat().st_size
            except OSError:
                pass
        hits = self._counter(self._load_manifest(), "hits")
        misses = self._counter(self._load_manifest(), "misses")
        lookups = hits + misses
        mtimes = [e["mtime"] for e in ents]
        return {
            "dir": str(self.dir),
            "entries": len(ents),
            "total_bytes": sum(e["size"] for e in ents),
            "oldest": _iso(min(mtimes)) if mtimes else None,
            "newest": _iso(max(mtimes)) if mtimes else None,
            "hits": hits,
            "misses": misses,
            "hit_rate": (hits / lookups) if lookups else None,
            "orphan_objects": len(orphs),
            "orphan_bytes": orph_bytes,
        }

    def _evict(self, key):
        """Delete one object file and its manifest record; return freed bytes."""
        freed = 0
        obj = self.object_path(key)
        try:
            freed = obj.stat().st_size
        except OSError:
            pass
        try:
            obj.unlink()
        except OSError:
            pass
        manifest = self._load_manifest()
        if manifest.pop(key, None) is not None:
            self._flush()
        return freed

    def prune(self, older_than=None, max_bytes=None):
        """Drop stale entries.

        older_than  seconds: remove tracked entries whose object file was
                    last used (mtime) before now - older_than.
        max_bytes   bytes: with the remaining objects sorted oldest-first,
                    evict until the total fits the budget (untracked orphan
                    objects count against the budget and go first among
                    equally old files).

        Returns {"removed": n, "bytes_freed": b}.
        """
        removed = 0
        freed = 0
        if older_than is not None:
            cutoff = time.time() - older_than
            for entry in self.entries():
                if entry["mtime"] < cutoff:
                    freed += self._evict(entry["key"])
                    removed += 1
        if max_bytes is not None:
            pool = []
            for f in self.objects():
                try:
                    st = f.stat()
                except OSError:
                    continue
                pool.append((st.st_mtime, f.name, st.st_size, f))
            pool.sort()
            total = sum(item[2] for item in pool)
            for _mtime, _name, size, f in pool:
                if total <= max_bytes:
                    break
                freed += self._evict(f.stem)
                total -= size
                removed += 1
        return {"removed": removed, "bytes_freed": freed}

    def verify(self):
        """Integrity check: recompute the sha256 of every object that stored
        one, remove corrupt objects (+ their manifest records), manifest
        records whose object vanished, and untracked orphan objects.  Legacy
        entries without a stored digest are counted but left alone.

        Returns {"checked", "corrupt", "missing", "orphan", "legacy",
                 "bytes_freed", "remaining"}.
        """
        manifest = self._load_manifest()
        tally = {"checked": 0, "corrupt": 0, "missing": 0, "orphan": 0,
                 "legacy": 0, "bytes_freed": 0}
        dirty = False
        for key in list(manifest):
            if key in COUNTER_KEYS or not isinstance(manifest[key], dict):
                continue
            obj = self.object_path(key)
            if not obj.is_file():
                manifest.pop(key)
                dirty = True
                tally["missing"] += 1
                continue
            tally["checked"] += 1
            want = manifest[key].get("sha256")
            if want is None:
                tally["legacy"] += 1
                continue
            try:
                got = hashlib.sha256(obj.read_bytes()).hexdigest()
            except OSError:
                got = None
            if got != want:
                try:
                    tally["bytes_freed"] += obj.stat().st_size
                except OSError:
                    pass
                try:
                    obj.unlink()
                except OSError:
                    pass
                manifest.pop(key)
                dirty = True
                tally["corrupt"] += 1
        for f in self.objects():
            if isinstance(manifest.get(f.stem), dict):
                continue
            try:
                tally["bytes_freed"] += f.stat().st_size
                f.unlink()
            except OSError:
                continue
            tally["orphan"] += 1
        if dirty:
            self._flush()
        tally["remaining"] = len(self.entries())
        return tally


def _iso(epoch):
    return time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(epoch))
