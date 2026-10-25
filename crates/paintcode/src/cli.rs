//! `paintcode SCENE -o OUT [-o OUT ...]`: the command line, for `python -m paintcode` and the
//! `paintcode` script. It only reads files, calls the engine and writes files.

use std::path::Path;
use std::time::Instant;

use serde_json::Value;

use crate::io::{image_to_png, laid_to_ply, petal_to_scene, png_to_image, scene_to_petal};
use crate::scene::{Error, Result, Scene};
use crate::{render_over, RenderOpts};

pub const USAGE: &str = "\
usage: paintcode SCENE -o OUT [-o OUT ...] [--base IMAGE] [--scale S] [--ss N]

Render a paintcode scene.

  SCENE           a .json scene, a .petal project, or - for JSON on stdin
  -o, --out OUT   output file; repeat for several formats, picked by extension:
                    .png    the rendered painting
                    .petal  the project: strokes, losslessly compressed, plus a small preview
                    .ply    every dab as a flat 3D Gaussian (3DGS layout, SH degree 0)
  --base IMAGE    paint on top of this PNG instead of a blank canvas (at its own size)
  --scale S       render larger; coordinates and brush sizes scale with it
  --ss N          supersampling factor (overrides canvas.supersample)
  -h, --help      show this message
";

const FORMATS: [&str; 3] = ["png", "petal", "ply"];

#[derive(Debug, Default, PartialEq)]
struct Args {
    scene: String,
    outs: Vec<String>,
    base: Option<String>,
    scale: Option<f64>,
    ss: Option<i64>,
}

fn parse_args(argv: &[String]) -> Result<Option<Args>> {
    let mut a = Args::default();
    let mut scene = None;
    let mut it = argv.iter();
    while let Some(arg) = it.next() {
        let mut value = |flag: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| Error(format!("{flag} needs a value")))
        };
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-o" | "--out" => a.outs.push(value(arg)?),
            "--base" => a.base = Some(value(arg)?),
            "--scale" => {
                let v = value(arg)?;
                a.scale = Some(
                    v.parse()
                        .map_err(|_| Error(format!("--scale {v}: not a number")))?,
                )
            }
            "--ss" => {
                let v = value(arg)?;
                a.ss = Some(
                    v.parse()
                        .map_err(|_| Error(format!("--ss {v}: not a whole number")))?,
                )
            }
            s if s.starts_with('-') && s != "-" => {
                return Err(Error(format!("unknown option {s}")))
            }
            s => match scene {
                None => scene = Some(s.to_string()),
                Some(_) => return Err(Error(format!("one scene at a time; also got {s}"))),
            },
        }
    }
    a.scene = scene.ok_or_else(|| Error("no SCENE given".into()))?;
    if a.outs.is_empty() {
        return Err(Error(
            "no output given; add -o OUT.png (or .petal, .ply)".into(),
        ));
    }
    let bad: Vec<&str> = a
        .outs
        .iter()
        .filter(|o| !FORMATS.contains(&ext(o).as_str()))
        .map(String::as_str)
        .collect();
    if !bad.is_empty() {
        return Err(Error(format!(
            "unknown output format {bad:?}; use .png, .petal or .ply"
        )));
    }
    Ok(Some(a))
}

fn ext(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn read(path: &str) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|e| Error(format!("{path}: {e}")))
}

fn write(path: &str, bytes: &[u8]) -> Result<()> {
    std::fs::write(path, bytes).map_err(|e| Error(format!("{path}: {e}")))
}

/// Read a scene as written: a .petal project, JSON on stdin (`-`), or a JSON file.
pub fn load_scene(path: &str) -> Result<Value> {
    let text = if path == "-" {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
            .map_err(|e| Error(format!("stdin: {e}")))?;
        s.into_bytes()
    } else if ext(path) == "petal" {
        return petal_to_scene(&read(path)?).map_err(|e| Error(format!("{path}: {e}")));
    } else {
        read(path)?
    };
    serde_json::from_slice(&text).map_err(|e| Error(format!("{path}: scene is not JSON: {e}")))
}

/// Run the command line. Returns the exit status; reports go to stdout, errors to stderr.
pub fn main(argv: &[String]) -> i32 {
    match run(argv) {
        Ok(lines) => {
            for l in lines {
                println!("{l}");
            }
            0
        }
        Err(e) => {
            eprintln!("paintcode: error: {e}");
            if e.0.starts_with("no ") || e.0.starts_with("unknown option") {
                eprint!("\n{USAGE}");
            }
            2
        }
    }
}

/// The work of [`main`], with the report as lines.
pub fn run(argv: &[String]) -> Result<Vec<String>> {
    let Some(a) = parse_args(argv)? else {
        return Ok(vec![USAGE.trim_end().to_string()]);
    };
    let value = load_scene(&a.scene)?;
    let scene = Scene::from_value(&value)?;
    let base = match &a.base {
        Some(p) => Some(png_to_image(&read(p)?).map_err(|e| Error(format!("{p}: {e}")))?),
        None => None,
    };
    let t = Instant::now();
    let want_ply = a.outs.iter().any(|o| ext(o) == "ply");
    let opts = RenderOpts {
        scale: a.scale.unwrap_or(1.0),
        supersample: a.ss,
        record: want_ply,
        ..RenderOpts::default()
    };
    let canvas = render_over(&scene, base.as_ref(), &opts)?;
    let image = canvas.to_rgb8();
    let mut report = Vec::new();
    for o in &a.outs {
        match ext(o).as_str() {
            "png" => {
                write(o, &image_to_png(&image))?;
                report.push(format!("{o}  {}x{}", canvas.width, canvas.height));
            }
            "petal" => {
                let bytes = scene_to_petal(&value, Some(&image)).map_err(Error)?;
                write(o, &bytes)?;
                report.push(format!(
                    "{o}  {:.1} KB, {} strokes",
                    bytes.len() as f64 / 1024.0,
                    scene.strokes.len()
                ));
            }
            _ => {
                // over a base image there is no single paper colour to note
                let bg = base.is_none().then_some(scene.canvas.background);
                let laid = canvas.laid.as_deref().unwrap_or(&[]);
                write(o, &laid_to_ply(laid, canvas.width, canvas.height, bg))?;
                report.push(format!("{o}  {} splats", laid.len()));
            }
        }
    }
    report.push(format!("rendered in {:.1}s", t.elapsed().as_secs_f64()));
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_the_options() {
        let a = parse_args(&args(
            "s.json -o a.png --out b.PLY --scale 2 --ss 3 --base x.png",
        ))
        .unwrap()
        .unwrap();
        assert_eq!(a.scene, "s.json");
        assert_eq!(a.outs, ["a.png", "b.PLY"]);
        assert_eq!(
            (a.scale, a.ss, a.base.as_deref()),
            (Some(2.0), Some(3), Some("x.png"))
        );
        assert_eq!(parse_args(&args("--help")).unwrap(), None);
    }

    #[test]
    fn says_what_is_wrong() {
        let e = |s: &str| parse_args(&args(s)).unwrap_err().0;
        assert!(e("s.json").contains("no output"));
        assert!(e("-o a.png").contains("no SCENE"));
        assert!(e("s.json -o a.jpg").contains("unknown output format"));
        assert!(e("s.json -o a.png --scale big").contains("not a number"));
        assert!(e("s.json -o a.png --frobnicate").contains("unknown option"));
        assert!(e("s.json -o").contains("needs a value"));
    }
}
