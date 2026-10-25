//! `paintcode._core`: the engine, seen from Python. This is the whole Python package; the files
//! in `python/paintcode/` only re-export it and describe it (`_core.pyi`).
//!
//! A scene is a JSON string or anything `json.dumps` takes (usually a dict). A base image is PNG
//! bytes or a path to a PNG.

use paintcode::render::RgbImage;
use paintcode::{Error, Flower, RenderOpts, io};
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

fn parse(py: Python<'_>, scene: &Bound<'_, PyAny>) -> PyResult<(Flower, serde_json::Value)> {
    let text = scene_text(py, scene)?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| PyValueError::new_err(format!("scene is not JSON: {e}")))?;
    let parsed = Flower::from_value(&value).map_err(value_error)?;
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
        supersample: supersample.unwrap_or(1),
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
            paintcode::render(&scene, &o)
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
    let paper = Some(scene.background);
    let ply = py
        .detach(|| {
            paintcode::render(&scene, &o).map(|c| {
                let laid = c.laid.as_deref().unwrap_or(&[]);
                paintcode::laid_to_ply(laid, c.width, c.height, paper)
            })
        })
        .map_err(value_error)?;
    Ok(PyBytes::new(py, &ply))
}

/// What goes into a .petal as its preview: a render of the scene, nothing, or a PNG already made.
#[derive(FromPyObject)]
enum Preview<'py> {
    Render(bool),
    Png(Bound<'py, PyBytes>),
}

/// Pack a scene as a .petal project (bytes): the strokes, losslessly compressed, never the dabs.
/// preview: True renders the scene for a small preview.png inside, False leaves it out, and PNG
/// bytes (the render you already have) are used as they are, without rendering again.
#[pyfunction]
#[pyo3(signature = (scene, *, preview = Preview::Render(true)))]
fn scene_to_petal<'py>(
    py: Python<'py>,
    scene: &Bound<'py, PyAny>,
    preview: Preview<'py>,
) -> PyResult<Bound<'py, PyBytes>> {
    let (parsed, value) = parse(py, scene)?;
    let given = match &preview {
        Preview::Png(b) => Some(
            io::png_to_image(b.as_bytes())
                .map_err(|e| PyValueError::new_err(format!("preview: {e}")))?,
        ),
        Preview::Render(_) => None,
    };
    let render = matches!(preview, Preview::Render(true));
    let petal = py
        .detach(|| {
            let image = match given {
                Some(img) => Some(img),
                None if render => {
                    Some(paintcode::render(&parsed, &RenderOpts::default())?.to_rgb8())
                }
                None => None,
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

#[pymodule]
#[pyo3(name = "_core")]
fn paintcode_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(render_png, m)?)?;
    m.add_function(wrap_pyfunction!(render_ply, m)?)?;
    m.add_function(wrap_pyfunction!(scene_to_petal, m)?)?;
    m.add_function(wrap_pyfunction!(petal_to_scene, m)?)?;
    // every preset with its fields, as a scene would write them
    let presets: serde_json::Map<String, serde_json::Value> = paintcode::flower::PRESET_NAMES
        .iter()
        .map(|n| {
            let b = paintcode::flower::preset(n).expect("a listed preset");
            (n.to_string(), serde_json::Value::Object(b.to_fields()))
        })
        .collect();
    let presets = m
        .py()
        .import("json")?
        .call_method1("loads", (serde_json::Value::Object(presets).to_string(),))?;
    m.add("PRESETS", presets)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
