"""paintcode: a small dab engine. Every brush mark is a row of anisotropic 2D Gaussians.

    import paintcode
    png = paintcode.render_png(scene)            # scene: a dict, or JSON text
    ply = paintcode.render_ply(scene)            # every dab as a 3D Gaussian splat
    petal = paintcode.scene_to_petal(scene)      # the project file: strokes, never dabs
    scene = paintcode.petal_to_scene(petal)

The engine is Rust (paintcode._core); this package is its Python face. See help(paintcode._core)
or the README for the scene format.
"""

from ._core import (
    PRESETS,
    __version__,
    petal_to_scene,
    render_f32,
    render_ply,
    render_png,
    scene_to_petal,
)

__all__ = ["PRESETS", "petal_to_scene", "render_f32", "render_ply", "render_png", "scene_to_petal"]
