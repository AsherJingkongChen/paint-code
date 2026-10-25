//! `paintcode._core`: the engine, seen from Python. This is the whole Python package; the files
//! in `python/paintcode/` only re-export it and describe it (`_core.pyi`).
//!
//! A flower is a dict (anything `json.dumps` takes). A load_ takes the path of a file; a save_
//! takes a pathname, the path without its suffix, and adds the suffix of what it writes. Both are a
//! str or an `os.PathLike`, and the files are read and written here, in Rust. A function with more than one parameter takes them
//! by keyword only.

use std::path::{Path, PathBuf};

use paintcode::{Error, Flower, RenderOpts, io};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use serde_json::Value;

fn value_error(e: Error) -> PyErr {
    PyValueError::new_err(e.0)
}

/// Write `bytes` at `pathname` plus `suffix`, and give back the path written.
fn write(pathname: PathBuf, suffix: &str, bytes: &[u8]) -> PyResult<PathBuf> {
    let mut path = pathname.into_os_string();
    path.push(suffix);
    let path = PathBuf::from(path);
    std::fs::write(&path, bytes)?;
    Ok(path)
}

/// A file's mistake, named with its path.
fn file_error(path: &Path, e: String) -> PyErr {
    PyValueError::new_err(format!("{}: {e}", path.display()))
}

/// A flower from its dict, checked: what the engine would refuse is refused here, before any file
/// is written or any pixel is painted.
fn to_flower(py: Python<'_>, flower: &Bound<'_, PyAny>) -> PyResult<Flower> {
    let text: String = py
        .import("json")?
        .call_method1("dumps", (flower,))?
        .extract()?;
    Flower::from_json(&text).map_err(value_error)
}

/// JSON to a dict, keys in their written order.
fn to_dict<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    py.import("json")?
        .call_method1("loads", (value.to_string(),))
}

fn opts(scale: f64, supersample: i64) -> RenderOpts {
    RenderOpts {
        scale,
        supersample,
        ..RenderOpts::default()
    }
}

/// The pixels you see, 8-bit RGB, and the flower on them if one was put there.
#[pyclass(frozen, module = "paintcode")]
struct Canvas(paintcode::Canvas);

#[pymethods]
impl Canvas {
    #[getter]
    fn width(&self) -> usize {
        self.0.width
    }

    #[getter]
    fn height(&self) -> usize {
        self.0.height
    }

    /// One pixel as (r, g, b), 0..255.
    #[pyo3(signature = (*, x, y))]
    fn peek(&self, x: usize, y: usize) -> PyResult<(u8, u8, u8)> {
        let (w, h) = (self.0.width, self.0.height);
        if x >= w || y >= h {
            return Err(PyValueError::new_err(format!(
                "pixel ({x}, {y}) is outside the {w}x{h} canvas"
            )));
        }
        let i = (y * w + x) * 3;
        let d = &self.0.pixels;
        Ok((d[i], d[i + 1], d[i + 2]))
    }

    /// A rectangle of this canvas as a canvas of its own. The flower stays behind: it painted the
    /// whole canvas, not this part.
    #[pyo3(signature = (*, x, y, width, height))]
    fn crop(&self, x: usize, y: usize, width: usize, height: usize) -> PyResult<Canvas> {
        let (w, h) = (self.0.width, self.0.height);
        let fits =
            |at: usize, len: usize, side: usize| at.checked_add(len).is_some_and(|e| e <= side);
        if width == 0 || height == 0 || !fits(x, width, w) || !fits(y, height, h) {
            return Err(PyValueError::new_err(format!(
                "a {width}x{height} crop at ({x}, {y}) does not fit in the {w}x{h} canvas"
            )));
        }
        let mut data = Vec::with_capacity(width * height * 3);
        for row in y..y + height {
            let i = (row * w + x) * 3;
            data.extend_from_slice(&self.0.pixels[i..i + width * 3]);
        }
        Ok(Canvas(paintcode::Canvas {
            width,
            height,
            pixels: data,
            flower: None,
        }))
    }

    /// Every pixel, row by row, as RGB bytes: `numpy.frombuffer(c.pixels(), "u1").reshape(
    /// c.height, c.width, 3)`.
    fn pixels<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &self.0.pixels)
    }

    /// The canvas as PNG bytes, with its flower if one is on it: what save_canvas writes.
    fn png<'py>(&self, py: Python<'py>) -> Bound<'py, PyBytes> {
        PyBytes::new(py, &io::encode_canvas(&self.0))
    }

    fn __repr__(&self) -> String {
        let on = if self.0.flower.is_some() {
            ", with its flower"
        } else {
            ""
        };
        format!("<Canvas {}x{}{on}>", self.0.width, self.0.height)
    }
}

/// Every dab a flower laid, in order: what coat_to_canvas lays again.
#[pyclass(frozen, module = "paintcode")]
struct Coat(paintcode::Coat);

#[pymethods]
impl Coat {
    /// The canvas it makes, in pixels.
    #[getter]
    fn width(&self) -> usize {
        self.0.width
    }

    #[getter]
    fn height(&self) -> usize {
        self.0.height
    }

    #[getter]
    fn supersample(&self) -> usize {
        self.0.supersample
    }

    /// The number of dabs.
    fn __len__(&self) -> usize {
        self.0.dabs.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "<Coat of {} dabs for a {}x{} canvas>",
            self.0.dabs.len(),
            self.0.width,
            self.0.height
        )
    }
}

// ------------------------------------------------------------------------------------- files

/// The flower in a `.flower.json`, or on a `.flower.png`. A PNG without a flower is an error.
#[pyfunction]
fn load_flower<'py>(py: Python<'py>, path: PathBuf) -> PyResult<Bound<'py, PyAny>> {
    let bytes = std::fs::read(&path)?;
    let flower = io::decode_flower(&bytes).map_err(|e| file_error(&path, e))?;
    to_dict(py, &flower.source)
}

/// Write a flower as JSON, one petal per line, at `pathname.flower.json`.
#[pyfunction]
#[pyo3(signature = (*, flower, pathname))]
fn save_flower(py: Python<'_>, flower: &Bound<'_, PyAny>, pathname: PathBuf) -> PyResult<PathBuf> {
    let flower = to_flower(py, flower)?;
    write(pathname, ".flower.json", &io::encode_flower(&flower))
}

/// The canvas in a PNG: only the pixels, whether or not a flower is on it (load_flower takes the
/// flower). Any PNG opens: grey, palette, alpha and 16-bit images become 8-bit RGB.
#[pyfunction]
fn load_canvas(path: PathBuf) -> PyResult<Canvas> {
    let bytes = std::fs::read(&path)?;
    let canvas = io::decode_canvas(&bytes).map_err(|e| file_error(&path, e))?;
    Ok(Canvas(paintcode::Canvas {
        flower: None,
        ..canvas
    }))
}

/// Write a canvas as a PNG at `pathname.png`, or with its flower on it at `pathname.flower.png`.
#[pyfunction]
#[pyo3(signature = (*, canvas, pathname))]
fn save_canvas(py: Python<'_>, canvas: PyRef<'_, Canvas>, pathname: PathBuf) -> PyResult<PathBuf> {
    let c: &Canvas = &canvas;
    let png = py.detach(|| io::encode_canvas(&c.0));
    let suffix = if c.0.flower.is_some() {
        ".flower.png"
    } else {
        ".png"
    };
    write(pathname, suffix, &png)
}

/// The coat in a `.coat.ply`. A 3DGS file from elsewhere is not a coat and is refused.
#[pyfunction]
fn load_coat(path: PathBuf) -> PyResult<Coat> {
    let bytes = std::fs::read(&path)?;
    io::decode_coat(&bytes)
        .map(Coat)
        .map_err(|e| file_error(&path, e))
}

/// Write a coat at `pathname.coat.ply`, a binary PLY in the 3DGS layout that any splat viewer
/// opens, with every dab kept exactly, so load_coat gives the same coat back.
#[pyfunction]
#[pyo3(signature = (*, coat, pathname))]
fn save_coat(py: Python<'_>, coat: PyRef<'_, Coat>, pathname: PathBuf) -> PyResult<PathBuf> {
    let coat: &paintcode::Coat = &coat.0;
    let ply = py.detach(|| io::encode_coat(coat));
    write(pathname, ".coat.ply", &ply)
}

// ------------------------------------------------------------------------------- transforms

/// Paint a flower: its canvas.
#[pyfunction]
#[pyo3(signature = (*, flower, scale = 1.0, supersample = 1))]
fn flower_to_canvas(
    py: Python<'_>,
    flower: &Bound<'_, PyAny>,
    scale: f64,
    supersample: i64,
) -> PyResult<Canvas> {
    let flower = to_flower(py, flower)?;
    let o = opts(scale, supersample);
    let canvas = py
        .detach(|| paintcode::flower_to_canvas(&flower, &o))
        .map_err(value_error)?;
    Ok(Canvas(canvas))
}

/// Paint a flower and keep every dab it lays: its coat.
#[pyfunction]
#[pyo3(signature = (*, flower, scale = 1.0, supersample = 1))]
fn flower_to_coat(
    py: Python<'_>,
    flower: &Bound<'_, PyAny>,
    scale: f64,
    supersample: i64,
) -> PyResult<Coat> {
    let flower = to_flower(py, flower)?;
    let o = opts(scale, supersample);
    py.detach(|| paintcode::flower_to_coat(&flower, &o))
        .map(Coat)
        .map_err(value_error)
}

/// Lay a coat's dabs again: the same canvas, bit for bit, as painting its flower.
#[pyfunction]
fn coat_to_canvas(py: Python<'_>, coat: PyRef<'_, Coat>) -> PyResult<Canvas> {
    let coat: &paintcode::Coat = &coat.0;
    let canvas = py
        .detach(|| paintcode::coat_to_canvas(coat))
        .map_err(value_error)?;
    Ok(Canvas(canvas))
}

/// The canvas with a flower on it, which save_canvas writes into the PNG. Any flower that was on
/// it is replaced.
#[pyfunction]
#[pyo3(signature = (*, flower, canvas))]
fn flower_on_canvas(
    py: Python<'_>,
    flower: &Bound<'_, PyAny>,
    canvas: PyRef<'_, Canvas>,
) -> PyResult<Canvas> {
    let flower = to_flower(py, flower)?;
    Ok(Canvas(paintcode::flower_on_canvas(
        &flower,
        canvas.0.clone(),
    )))
}

/// The flower on a canvas. A canvas without one is an error.
#[pyfunction]
fn flower_off_canvas<'py>(
    py: Python<'py>,
    canvas: PyRef<'py, Canvas>,
) -> PyResult<Bound<'py, PyAny>> {
    let flower = paintcode::flower_off_canvas(&canvas.0).map_err(value_error)?;
    to_dict(py, &flower.source)
}

#[pymodule]
#[pyo3(name = "_core")]
fn paintcode_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Canvas>()?;
    m.add_class::<Coat>()?;
    m.add_function(wrap_pyfunction!(load_flower, m)?)?;
    m.add_function(wrap_pyfunction!(save_flower, m)?)?;
    m.add_function(wrap_pyfunction!(load_canvas, m)?)?;
    m.add_function(wrap_pyfunction!(save_canvas, m)?)?;
    m.add_function(wrap_pyfunction!(load_coat, m)?)?;
    m.add_function(wrap_pyfunction!(save_coat, m)?)?;
    m.add_function(wrap_pyfunction!(flower_to_canvas, m)?)?;
    m.add_function(wrap_pyfunction!(flower_to_coat, m)?)?;
    m.add_function(wrap_pyfunction!(coat_to_canvas, m)?)?;
    m.add_function(wrap_pyfunction!(flower_on_canvas, m)?)?;
    m.add_function(wrap_pyfunction!(flower_off_canvas, m)?)?;
    // every brush with its fields, as a flower would write them
    let brushes: serde_json::Map<String, Value> = paintcode::flower::BRUSH_NAMES
        .iter()
        .map(|n| {
            let b = paintcode::flower::brush(n).expect("a listed brush");
            (n.to_string(), Value::Object(b.to_fields()))
        })
        .collect();
    m.add("BRUSHES", to_dict(m.py(), &Value::Object(brushes))?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
