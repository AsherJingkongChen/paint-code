import math

import numpy as np
import pytest

from paintcode import Brush, Canvas, get_brush, paint_stroke, render, samples


def scene(**kw):
    s = {"canvas": {"width": 160, "height": 80, "seed": 3},
         "strokes": [{"brush": "filbert", "color": "#c8782a",
                      "points": [[10, 40, 0.2], [60, 20, 1.0], [110, 60, 0.7], [150, 40, 0.1]]}]}
    s.update(kw)
    return s


def test_same_scene_same_pixels():
    a = render(scene()).img
    b = render(scene()).img
    assert np.array_equal(a, b)


def test_seed_changes_jitter():
    s2 = scene(canvas={"width": 160, "height": 80, "seed": 4})
    assert not np.array_equal(render(scene()).img, render(s2).img)


def test_duplicate_points_do_not_change_the_stroke():
    pts = [[10, 40], [60, 40], [110, 40], [150, 40]]
    dup = [[10, 40], [60, 40], [60, 40], [110, 40], [110, 40], [110, 40], [150, 40]]
    b = get_brush("ink")
    c1, c2 = Canvas(160, 80, seed=1), Canvas(160, 80, seed=1)
    paint_stroke(c1, b, "#000", pts)
    paint_stroke(c2, b, "#000", dup)
    assert np.allclose(c1.img, c2.img)


def test_joints_keep_the_stroke_direction():
    # the paint-mcp bug: a repeated joint point got direction atan2(0, 0) = 0
    b = Brush(size=6, aspect=0.4)
    pts = [[10, 10], [30, 30], [30, 30], [50, 50], [70, 70]]
    dirs = [d for *_, d, _t in samples(pts, b, smooth=False)]
    assert dirs and all(abs(d - math.pi / 4) < 1e-6 for d in dirs)


def test_pressure_grows_radius_and_flow():
    b = get_brush("round")
    ps = np.linspace(0, 1, 11)
    r = [b.radius(p) for p in ps]
    f = [b.flow_at(p) for p in ps]
    assert all(x < y for x, y in zip(r, r[1:]))
    assert all(x < y for x, y in zip(f, f[1:]))
    assert math.isclose(r[0], b.size * b.min_size)


def test_single_point_is_one_dab():
    c = Canvas(40, 40, record_dabs=True)
    assert paint_stroke(c, get_brush("round"), "#f00", [[20, 20, 0.8]]) == 1
    assert len(c.dabs) == 1


def test_pickup_does_not_depend_on_dab_spacing():
    def end_colour(spacing):
        c = Canvas(300, 40, background="#ffffff", seed=0)
        b = Brush(size=8, flow=1.0, hardness=0.8, pickup=0.05, spacing=spacing)
        c.img[:, 150:] = 0.0   # wet black paint already on the right half
        c.wet[:, 150:] = 1.0
        paint_stroke(c, b, "#ff0000", [[10, 20], [290, 20]])
        return c.last_carried
    a, b = end_colour(0.08), end_colour(0.3)
    assert np.allclose(a, b, atol=0.06)


def test_scale_renders_larger():
    c = render(scene(), scale=2)
    assert (c.width, c.height) == (320, 160)


def test_unknown_brush_field_is_reported():
    s = scene()
    s["strokes"][0]["sise"] = 4
    with pytest.raises(ValueError, match="stroke 0"):
        render(s)


def test_dab_is_a_gaussian():
    c = Canvas(64, 64, background="#000000")
    c.stamp(32.0, 32.0, 6.0, 6.0, 0.0, np.array([1, 1, 1], np.float32), 1.0)
    row = c.img[31, :, 0]
    # one sigma from the centre (pixel centres at x + 0.5) the value is exp(-0.5)
    assert math.isclose(row[32 + 6 - 1] , math.exp(-0.5 * ((5.5 / 6) ** 2 + (0.5 / 6) ** 2)), rel_tol=1e-4)


def test_dry_background_is_not_picked_up():
    c = Canvas(300, 40, background="#f4efe6")
    paint_stroke(c, Brush(size=8, pickup=0.5), "#2b2d42", [[10, 20], [290, 20]])
    assert np.allclose(c.last_carried, np.array([0x2b, 0x2d, 0x42]) / 255, atol=1e-6)


def test_smudge_starts_with_the_colour_under_it():
    c = Canvas(100, 40, background="#ffffff")
    c.img[:, :50] = [1.0, 0.0, 0.0]
    c.wet[:, :50] = 1.0
    paint_stroke(c, get_brush("blender"), None, [[20, 20], [80, 20]])
    assert c.img[20, 60, 0] > 0.9 and c.img[20, 60, 1] < 0.9   # red dragged into the white


# ---------------------------------------------------------------- exports and sharpness

def test_hardness_sharpens_the_edge():
    def edge(h):
        c = Canvas(200, 60, background="#000000")
        paint_stroke(c, Brush(size=18, aspect=0.5, hardness=h), "#ffffff", [[20, 30], [180, 30]])
        col = c.img[:30, 100, 0]
        return int(np.argmax(col > 0.9) - np.argmax(col > 0.1))
    assert edge(0.0) > edge(0.6) > edge(0.9)


def test_supersampling_keeps_the_same_painting():
    a = render(scene()).img
    b = render(scene(), supersample=2).img
    assert a.shape == b.shape
    assert np.abs(a - b).mean() < 0.01


def test_petal_round_trip(tmp_path):
    from paintcode import project
    s = scene()
    project.save(tmp_path / "x.petal", s, preview=render(s).to_image())
    assert project.load(tmp_path / "x.petal") == s
    assert np.array_equal(render(project.load(tmp_path / "x.petal")).img, render(s).img)


def test_ply_is_the_painting(tmp_path):
    """A soft painting exported as 3DGS splats renders back to the same pixels."""
    from paintcode import ply
    s = {"canvas": {"width": 120, "height": 90, "background": "#203040", "seed": 5},
         "strokes": [
             {"brush": "round", "hardness": 0, "color": "#e0a040", "points": [[10, 20, 0.2], [60, 70, 1], [110, 30, 0.4]]},
             {"brush": "filbert", "hardness": 0, "grain": 0, "color": "#40a0e0", "points": [[10, 80], [110, 10]]},
             {"brush": "ink", "hardness": 0, "color": "#ffffff", "points": [[20, 45, 0.3], [100, 50, 1]]},
             {"brush": "blender", "color": None, "points": [[30, 30], [90, 60]]},
         ]}
    c = render(s, record_dabs=True)
    ply.write(tmp_path / "x.ply", c.dabs, c.width, c.height, background=[0x20 / 255, 0x30 / 255, 0x40 / 255])
    back = ply.render_splats(tmp_path / "x.ply")
    assert np.abs(back - c.img).max() < 2 / 255


def test_same_size_looks_the_same_at_any_hardness():
    widths = []
    for h in (0.0, 0.5, 0.8, 0.95):
        c = Canvas(200, 80, background="#000000")
        paint_stroke(c, Brush(size=16, hardness=h), "#ffffff", [[20, 40], [180, 40]])
        widths.append(int((c.img[:, 100, 0] > 0.5).sum()))
    assert max(widths) - min(widths) <= 0.2 * max(widths)


# ---------------------------------------------------------------- random numbers

def test_rng_spec_is_pinned():
    """These values are part of the format: the Rust core checks the same numbers."""
    from paintcode import rng
    assert rng.key(24, 1, 0, 0, 0) == 0x45C276EDBC638DF7
    assert rng.key(-1, 2, 3) == 0x48141E3702D4765A
    assert rng.uniform(24, 1, 0, 0, 0) == 0.27249854377840965
    prefix = rng.key(7, rng.GRAIN, 3000, 0)
    assert prefix == 0x731157264855CD28
    grid = rng.uniform_grid(prefix, 4)
    assert grid.dtype == np.float32
    assert grid.tolist() == [0.47280991077423096, 0.3959234356880188, 0.8277716636657715, 0.01953345537185669]
    assert [int(v * (1 << 24)) for v in rng.uniform_grid(prefix, 8)] == [rng.mix(prefix ^ i) >> 40 for i in range(8)]


def test_editing_a_stroke_leaves_the_others_jitter_alone():
    """Jitter is keyed by stroke and dab, so a longer first stroke does not move the second."""
    second = {"brush": "flat", "color": "#40a0e0", "points": [[20, 60], [140, 60]]}
    short = {"brush": "filbert", "color": "#c8782a", "points": [[10, 20], [80, 20]]}
    long = {"brush": "filbert", "color": "#c8782a", "points": [[10, 20], [80, 20], [150, 30]]}
    dabs = []
    for first in (short, long):
        c = render({"canvas": {"width": 160, "height": 80, "seed": 9}, "strokes": [first, second]}, record_dabs=True)
        n_first = sum(1 for d in c.dabs if abs(d["y"] - 60) > 15)
        dabs.append([(d["x"], d["y"], d["sx"], d["theta"]) for d in c.dabs[n_first:]])
    assert dabs[0] == dabs[1]


def test_pixel_arithmetic_stays_float32():
    """Stroke samples are numpy float64 scalars; they must not drag the dab maths into float64."""
    col = np.array([0.2, 0.5, 0.9], np.float32)
    args = (40.3, 21.7, 5.2, 2.1, 0.7, col, 0.8, 0.6)
    a, b = Canvas(80, 40), Canvas(80, 40)
    a.stamp(*[np.float64(v) if isinstance(v, float) else v for v in args])
    b.stamp(*args)
    assert a.img.dtype == np.float32 and np.array_equal(a.img, b.img)
