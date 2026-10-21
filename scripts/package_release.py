#!/usr/bin/env python3
"""Deterministically pack one already-staged release directory.

Usage: package_release.py <out.tar.gz> <staged-dir>

The archive root is the staged dir's basename; every entry carries epoch
mtimes, root owner, and sorted order, so the same inputs produce
byte-identical tarballs. __pycache__ and .pyc files are skipped.
"""
import os
import sys
import tarfile
from pathlib import Path


def main() -> None:
    out, staged = Path(sys.argv[1]), Path(sys.argv[2])
    name = staged.name

    files = sorted(
        p for p in staged.rglob("*")
        if p.is_file() and "__pycache__" not in p.parts and p.suffix != ".pyc"
    )

    with tarfile.open(out, "w:gz") as tf:
        for p in files:
            rel = f"{name}/{p.relative_to(staged)}"
            ti = tf.gettarinfo(str(p), arcname=rel)
            ti.uid = ti.gid = 0
            ti.uname = ti.gname = "root"
            ti.mtime = 0
            ti.mode = 0o755 if os.access(p, os.X_OK) else 0o644
            with open(p, "rb") as fh:
                tf.addfile(ti, fh)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
