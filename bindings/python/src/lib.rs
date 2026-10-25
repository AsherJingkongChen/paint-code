//! `paintcode._core`: the engine, seen from Python. This is the whole Python package; the files
//! in `python/paintcode/` only re-export it and describe it (`_core.pyi`).
//!
//! A scene is a JSON string or anything `json.dumps` takes (usually a dict). A base image is PNG
//! bytes or a path to a PNG.

use paintcode::render::RgbImage;
use paintcode::{io, Error, RenderOpts, Scene};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyString};

fn value_error(e: Error) -> PyErr {
    PyValueError::new_err(e.0)
}

/// The scene as JSON text: a str is taken as it is, anything else goes through `json.dumps`.
fn scene_text(py: Python<'_>, scene: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(s) = scene.cast::<PyString>() {
        return Ok(s.to_str()?.to_owned());
    }
    py.import("json")?
        .call_method1("dumps", (scene,))?
        .extract::<String>()
}

fn parse(py: Python<'_>, scene: &Bound<'_, PyAny>) -> PyResult<(Scene, serde_json::Value)> {
    let text = scene_text(py, scene)?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| PyValueError::new_err(format!("scene is not JSON: {e}")))?;
    let parsed = Scene::from_value(&value).map_err(value_error)?;
    Ok((parsed, value))
}

/// A base image from PNG bytes, or from a path (str or os.PathLike) to a PNG file.
fn base_image(py: Python<'_>, base: Option<&Bound<'_, PyAny>>) -> PyResult<Option<RgbImage>> {
    let Some(base) = base else { return Ok(None) };
    let bytes = match base.cast::<PyBytes>() {
        Ok(b) => b.as_bytes().to_vec(),
        Err(_) => {
            let path: String = py
                .import("os")?
                .call_method1("fspath", (base,))?
                .extract()?;
            std::fs::read(&path)?
        }
    };
    io::png_to_image(&bytes)
        .map(Some)
        .map_err(|e| PyValueError::new_err(format!("base image: {e}")))
}

fn opts(scale: f64, supersample: Option<i64>, record: bool) -> RenderOpts {
    RenderOpts {
        scale,
        supersample,
        record,
        ..RenderOpts::default()
    }
}

/// Render a scene to PNG bytes.
///
/// scale renders larger (coordinates and brush sizes scale together); supersample paints at N
/// times the size and box-filters down (default: canvas.supersample); base paints over a PNG
/// (bytes or a path) at its own size.
#[pyfunction]
#[pyo3(signature = (scene, *, scale = 1.0, supersample = None, base = None))]
fn render_png<'py>(
    py: Python<'py>,
    scene: &Bound<'py, PyAny>,
    scale: f64,
    supersample: Option<i64>,
    base: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyBytes>> {
    let (scene, _) = parse(py, scene)?;
    let base = base_image(py, base)?;
    let o = opts(scale, supersample, false);
    let png = py
        .detach(|| {
            paintcode::render_over(&scene, base.as_ref(), &o)
                .map(|c| paintcode::image_to_png(&c.to_rgb8()))
        })
        .map_err(value_error)?;
    Ok(PyBytes::new(py, &png))
}

/// Render a scene and export every dab as a flat 3D Gaussian: PLY bytes in the 3DGS layout.
/// Takes the same options as render_png.
#[pyfunction]
#[pyo3(signature = (scene, *, scale = 1.0, supersample = None, base = None))]
fn render_ply<'py>(
    py: Python<'py>,
    scene: &Bound<'py, PyAny>,
    scale: f64,
    supersample: Option<i64>,
    base: Option<&Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyBytes>> {
    let (scene, _) = parse(py, scene)?;
    let base = base_image(py, base)?;
    let o = opts(scale, supersample, true);
    let paper = base.is_none().then_some(scene.canvas.background);
    let ply = py
        .detach(|| {
            paintcode::render_over(&scene, base.as_ref(), &o).map(|c| {
                let laid = c.laid.as_deref().unwrap_or(&[]);
                paintcode::laid_to_ply(laid, c.width, c.height, paper)
            })
        })
        .map_err(value_error)?;
    Ok(PyBytes::new(py, &ply))
}

/// Render a scene to its float canvas: (width, height, bytes), the bytes being height x width x 3
/// little-endian float32 in 0..1. For checking pixels without 8-bit rounding.
#[pyfunction]
#[pyo3(signature = (scene, *, scale = 1.0, supersample = None, base = None))]
fn render_f32<'py>(
    py: Python<'py>,
    scene: &Bound<'py, PyAny>,
    scale: f64,
    supersample: Option<i64>,
    base: Option<&Bound<'py, PyAny>>,
) -> PyResult<(usize, usize, Bound<'py, PyBytes>)> {
    let (scene, _) = parse(py, scene)?;
    let base = base_image(py, base)?;
    let o = opts(scale, supersample, false);
    let canvas = py
        .detach(|| paintcode::render_over(&scene, base.as_ref(), &o))
        .map_err(value_error)?;
    let bytes: Vec<u8> = canvas.img.iter().flat_map(|v| v.to_le_bytes()).collect();
    Ok((canvas.width, canvas.height, PyBytes::new(py, &bytes)))
}

/// Pack a scene as a .petal project (bytes): the strokes, losslessly compressed, never the dabs.
/// With preview, the scene is rendered once for a small preview.png inside.
#[pyfunction]
#[pyo3(signature = (scene, *, preview = true))]
fn scene_to_petal<'py>(
    py: Python<'py>,
    scene: &Bound<'py, PyAny>,
    preview: bool,
) -> PyResult<Bound<'py, PyBytes>> {
    let (parsed, value) = parse(py, scene)?;
    let petal = py
        .detach(|| {
            let image = if preview {
                Some(paintcode::render(&parsed, &RenderOpts::default())?.to_rgb8())
            } else {
                None
            };
            paintcode::scene_to_petal(&value, image.as_ref()).map_err(Error)
        })
        .map_err(value_error)?;
    Ok(PyBytes::new(py, &petal))
}

/// The scene inside a .petal project (bytes), as a dict with its keys in their written order.
#[pyfunction]
fn petal_to_scene<'py>(py: Python<'py>, petal: &[u8]) -> PyResult<Bound<'py, PyAny>> {
    let value = paintcode::petal_to_scene(petal).map_err(PyValueError::new_err)?;
    py.import("json")?
        .call_method1("loads", (value.to_string(),))
}

/// The command line, `paintcode SCENE -o OUT ...`. argv defaults to sys.argv[1:]; returns the
/// exit status.
#[pyfunction]
#[pyo3(signature = (argv = None))]
fn main(py: Python<'_>, argv: Option<Vec<String>>) -> PyResult<i32> {
    let argv = match argv {
        Some(a) => a,
        None => {
            let all: Vec<String> = py.import("sys")?.getattr("argv")?.extract()?;
            all.into_iter().skip(1).collect()
        }
    };
    Ok(py.detach(|| paintcode::cli::main(&argv)))
}

#[pymodule]
#[pyo3(name = "_core")]
fn paintcode_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render_png, m)?)?;
    m.add_function(wrap_pyfunction!(render_ply, m)?)?;
    m.add_function(wrap_pyfunction!(render_f32, m)?)?;
    m.add_function(wrap_pyfunction!(scene_to_petal, m)?)?;
    m.add_function(wrap_pyfunction!(petal_to_scene, m)?)?;
    m.add_function(wrap_pyfunction!(main, m)?)?;
    m.add("PRESETS", paintcode::scene::PRESET_NAMES.to_vec())?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
