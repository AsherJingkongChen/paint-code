//! Modules may only use the ones below them (see the table in src/lib.rs). One crate cannot have
//! cargo enforce this, so this test does: a reference back down the list is a loud failure.

use std::collections::BTreeSet;
use std::path::Path;

const ALLOWED: [(&str, &[&str]); 5] = [
    ("math", &[]),
    ("flower", &["math"]),
    ("render", &["math"]),
    ("coat", &["math", "flower", "render"]),
    ("io", &["render"]),
];

fn sources(m: &str, src: &Path) -> Vec<String> {
    let mut out = vec![std::fs::read_to_string(src.join(format!("{m}.rs"))).unwrap()];
    if let Ok(dir) = std::fs::read_dir(src.join(m)) {
        for e in dir {
            out.push(std::fs::read_to_string(e.unwrap().path()).unwrap());
        }
    }
    out
}

#[test]
fn modules_only_use_the_ones_below_them() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let names: Vec<&str> = ALLOWED.iter().map(|(m, _)| *m).collect();
    for (m, allowed) in ALLOWED {
        let mut used = BTreeSet::new();
        for text in sources(m, &src) {
            for line in text.lines().filter(|l| !l.trim_start().starts_with("//")) {
                for other in &names {
                    if *other != m && line.contains(&format!("crate::{other}")) {
                        used.insert(*other);
                    }
                }
            }
        }
        let bad: Vec<_> = used.iter().filter(|u| !allowed.contains(u)).collect();
        assert!(
            bad.is_empty(),
            "module `{m}` uses {bad:?}, but may only use {allowed:?}"
        );
    }
}
