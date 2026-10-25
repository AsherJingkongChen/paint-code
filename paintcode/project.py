"""The .petal project file: the strokes of a painting, losslessly compressed.

A .petal is a plain ZIP (Deflate) holding
    meta.json    format name, version, engine version
    scene.json   the scene exactly as written: canvas, brushes, strokes
    preview.png  optional small render for file browsers

It stores strokes, never dabs: the engine regenerates every dab from the scene and its seed,
so the strokes are the whole painting.
"""

from __future__ import annotations

import io
import json
import zipfile
from pathlib import Path

FORMAT = "petal"
VERSION = 1


def save(path, scene: dict, preview=None, engine_version: str | None = None) -> None:
    """Write a .petal. preview is an optional PIL image (it is shrunk to fit 256 px)."""
    from . import __version__
    meta = {"format": FORMAT, "version": VERSION, "engine": f"paintcode {engine_version or __version__}"}
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        z.writestr("meta.json", json.dumps(meta, indent=1))
        z.writestr("scene.json", json.dumps(scene, separators=(",", ":"), ensure_ascii=False))
        if preview is not None:
            im = preview.copy()
            im.thumbnail((256, 256))
            buf = io.BytesIO()
            im.save(buf, "PNG", optimize=True)
            z.writestr("preview.png", buf.getvalue(), compress_type=zipfile.ZIP_STORED)


def load(path) -> dict:
    with zipfile.ZipFile(path) as z:
        names = set(z.namelist())
        if "scene.json" not in names:
            raise ValueError(f"{path}: not a .petal file (no scene.json)")
        if "meta.json" in names:
            meta = json.loads(z.read("meta.json"))
            if meta.get("format") != FORMAT or int(meta.get("version", 0)) > VERSION:
                raise ValueError(f"{path}: unsupported project format {meta}")
        return json.loads(z.read("scene.json"))


def is_project(path) -> bool:
    return Path(path).suffix.lower() == ".petal"
