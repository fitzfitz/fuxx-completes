//! Turns the project mappings in `projects/*.toml` into static Rust tables
//! (`$OUT_DIR/projects_table.rs`, included by `src/projects/mod.rs`). Bad data fails the build
//! with the file and the entry, never the user (design §5.4, M1b).

use serde::Deserialize;
use std::collections::HashSet;
use std::fmt::Write;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    project: Vec<Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    markers: Vec<String>,
    #[serde(default)]
    requires: Vec<String>,
    #[serde(default)]
    requires_any: Vec<String>,
    contains: Option<Contains>,
    #[serde(default)]
    beats: Vec<String>,
    #[serde(default)]
    commands: Vec<String>,
    #[serde(default)]
    from: Vec<Source>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Contains {
    file: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    reader: String,
    files: Vec<String>,
    #[serde(default)]
    path: String,
    templates: Vec<String>,
}

fn strs(v: &[String]) -> String {
    let inner: Vec<String> = v.iter().map(|s| format!("{s:?}")).collect();
    format!("&[{}]", inner.join(", "))
}

/// `*.ext` becomes `Ext(".ext")`; anything else with a `*` is refused.
fn ext(pattern: &str) -> Option<&str> {
    let e = pattern.strip_prefix('*')?;
    (e.starts_with('.') && e.len() > 1 && !e[1..].contains(['*', '.', '/'])).then_some(e)
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../projects");
    println!("cargo::rerun-if-changed={}", dir.display());
    let mut names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("projects/: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    names.sort();
    let mut entries: Vec<(String, Entry)> = Vec::new();
    for path in &names {
        println!("cargo::rerun-if-changed={}", path.display());
        let text = std::fs::read_to_string(path).unwrap();
        let file: File =
            toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let fname = path.file_name().unwrap().to_string_lossy().into_owned();
        entries.extend(file.project.into_iter().map(|e| (fname.clone(), e)));
    }
    let ids: HashSet<&str> = entries.iter().map(|(_, e)| e.id.as_str()).collect();
    let mut seen = HashSet::new();
    let mut out = String::from("pub static PROJECTS: &[ProjectType] = &[\n");
    for (fname, e) in &entries {
        let at = format!("{fname}: project {:?}", e.id);
        assert!(seen.insert(e.id.clone()), "{at}: duplicate id");
        assert!(!e.markers.is_empty(), "{at}: no markers");
        let markers: Vec<String> = e
            .markers
            .iter()
            .map(|m| {
                if m.contains('*') {
                    let x = ext(m).unwrap_or_else(|| panic!("{at}: marker {m:?} is not *.ext"));
                    format!("Marker::Ext({x:?})")
                } else {
                    format!("Marker::Name({m:?})")
                }
            })
            .collect();
        for b in &e.beats {
            assert!(
                ids.contains(b.as_str()) && *b != e.id,
                "{at}: beats unknown {b:?}"
            );
        }
        let contains = match &e.contains {
            Some(c) => format!(
                "Some(Contains {{ file: {:?}, text: {:?} }})",
                c.file, c.text
            ),
            None => "None".into(),
        };
        let mut from = Vec::new();
        for s in &e.from {
            let reader = match s.reader.as_str() {
                "json_keys" => "JsonKeys",
                "toml_keys" => "TomlKeys",
                "make_targets" => "MakeTargets",
                "just_recipes" => "JustRecipes",
                "yaml_keys" => "YamlKeys",
                "file_names" => "FileNames",
                r => panic!("{at}: unknown reader {r:?}"),
            };
            assert!(!s.files.is_empty(), "{at}: a source without files");
            assert!(!s.templates.is_empty(), "{at}: a source without templates");
            for t in &s.templates {
                assert!(t.contains("{}"), "{at}: template {t:?} has no {{}}");
            }
            let needs_path = matches!(reader, "JsonKeys" | "TomlKeys" | "YamlKeys");
            assert_eq!(
                needs_path,
                !s.path.is_empty(),
                "{at}: path for {}",
                s.reader
            );
            if reader == "FileNames" {
                for f in &s.files {
                    assert!(
                        ext(f).is_some(),
                        "{at}: file_names pattern {f:?} is not *.ext"
                    );
                }
            }
            from.push(format!(
                "Source {{ reader: Reader::{reader}, files: {}, path: {:?}, templates: {} }}",
                strs(&s.files),
                s.path,
                strs(&s.templates)
            ));
        }
        writeln!(
            out,
            "    ProjectType {{ id: {:?}, markers: &[{}], requires: {}, requires_any: {}, \
             contains: {contains}, beats: {}, commands: {}, from: &[{}] }},",
            e.id,
            markers.join(", "),
            strs(&e.requires),
            strs(&e.requires_any),
            strs(&e.beats),
            strs(&e.commands),
            from.join(", ")
        )
        .unwrap();
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("projects_table.rs");
    std::fs::write(dest, out).unwrap();
}
