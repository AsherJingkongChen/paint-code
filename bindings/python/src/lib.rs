//! `paintcode._core`: the Rust core, seen from Python. Scene JSON in, bytes out.
//! The friendly wrapper is `paintcode/core.py`.

use paintcode::{RenderOpts, Scene};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

fn opts(scale: f64, supersample: Option<i64>) -> RenderOpts {
    RenderOpts {
        scale,
        supersample,
        ..RenderOpts::default()
    }
}

fn parse(scene_json: &str) -> PyResult<Scene> {
    Scene::from_json(scene_json).map_err(|e| PyValueError::new_err(e.0))
}

/// Render a scene (a JSON string) to PNG bytes.
#[pyfunction]
#[pyo3(signature = (scene_json, *, scale = 1.0, supersample = None))]
fn render_png<'py>(
    py: Python<'py>,
    scene_json: &str,
    scale: f64,
    supersample: Option<i64>,
) -> PyResult<Bound<'py, PyBytes>> {
    let scene = parse(scene_json)?;
    let o = opts(scale, supersample);
    let png =
        py.detach(|| paintcode::render(&scene, &o).map(|c| paintcode::image_to_png(&c.to_rgb8())));
    let png = png.map_err(|e| PyValueError::new_err(e.0))?;
    Ok(PyBytes::new(py, &png))
}

/// Render a scene to its float canvas: (width, height, bytes of height x width x 3 float32).
/// For checking the core against the Python engine.
#[pyfunction]
#[pyo3(signature = (scene_json, *, scale = 1.0, supersample = None))]
fn render_f32<'py>(
    py: Python<'py>,
    scene_json: &str,
    scale: f64,
    supersample: Option<i64>,
) -> PyResult<(usize, usize, Bound<'py, PyBytes>)> {
    let scene = parse(scene_json)?;
    let o = opts(scale, supersample);
    let canvas = py
        .detach(|| paintcode::render(&scene, &o))
        .map_err(|e| PyValueError::new_err(e.0))?;
    let bytes: Vec<u8> = canvas.img.iter().flat_map(|v| v.to_le_bytes()).collect();
    Ok((canvas.width, canvas.height, PyBytes::new(py, &bytes)))
}

#[pymodule]
#[pyo3(name = "_core")]
fn paintcode_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render_png, m)?)?;
    m.add_function(wrap_pyfunction!(render_f32, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
