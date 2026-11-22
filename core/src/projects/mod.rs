//! Project knowledge (`docs/design.md` §5.4, M1b): what kind of project a folder is, from our
//! own mapping data (`projects/*.toml`, compiled in by `build.rs`), and the commands it offers.
//! Nothing in a project is ever run; folders are listed and small files read.

/// A marker: a file or folder name, or `*.ext` (any name ending in `.ext`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Name(&'static str),
    /// The suffix, dot included (`*.csproj` is `Ext(".csproj")`).
    Ext(&'static str),
}

/// The built-in readers (spec §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reader {
    JsonKeys,
    TomlKeys,
    MakeTargets,
    JustRecipes,
    YamlKeys,
    FileNames,
}

/// Where project-specific commands come from: a reader over the first of `files` that exists
/// (for `FileNames`, a `*.ext` pattern matched against the listing), and the templates each
/// name goes through (`{}` is the name).
#[derive(Debug, Clone, Copy)]
pub struct Source {
    pub reader: Reader,
    pub files: &'static [&'static str],
    pub path: &'static str,
    pub templates: &'static [&'static str],
}

/// A file that must contain a text for the type to match (flutter's `sdk: flutter`).
#[derive(Debug, Clone, Copy)]
pub struct Contains {
    pub file: &'static str,
    pub text: &'static str,
}

/// One project type from the mapping data.
#[derive(Debug, Clone, Copy)]
pub struct ProjectType {
    pub id: &'static str,
    /// Any of these in the folder's listing.
    pub markers: &'static [Marker],
    /// All of these present (relative paths).
    pub requires: &'static [&'static str],
    /// When not empty, one of these present.
    pub requires_any: &'static [&'static str],
    pub contains: Option<Contains>,
    /// Types this one drops when both match in the same folder.
    pub beats: &'static [&'static str],
    /// Fixed commands, everyday ones first.
    pub commands: &'static [&'static str],
    pub from: &'static [Source],
}

pub mod detect;
pub mod readers;

// The table generated from `projects/*.toml` by `build.rs`.
include!(concat!(env!("OUT_DIR"), "/projects_table.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const PLANNED: [&str; 39] = [
        "npm",
        "pnpm",
        "yarn",
        "bun",
        "deno",
        "expo",
        "uv",
        "poetry",
        "pip",
        "django",
        "cargo",
        "cmake",
        "cmake-presets",
        "meson",
        "make",
        "dotnet",
        "flutter",
        "dart",
        "android",
        "gradlew",
        "gradle",
        "maven",
        "xcode",
        "swiftpm",
        "cocoapods",
        "go",
        "ruby",
        "rails",
        "composer",
        "laravel",
        "just",
        "task",
        "compose",
        "docker",
        "terraform",
        "helm",
        "mix",
        "zig",
        "nix",
    ];

    fn get(id: &str) -> &'static ProjectType {
        PROJECTS.iter().find(|p| p.id == id).unwrap()
    }

    #[test]
    fn table_has_every_planned_ecosystem() {
        let ids: HashSet<&str> = PROJECTS.iter().map(|p| p.id).collect();
        let planned: HashSet<&str> = PLANNED.into_iter().collect();
        assert_eq!(ids, planned);
    }

    #[test]
    fn ids_are_unique_and_beats_are_known() {
        let mut seen = HashSet::new();
        for p in PROJECTS {
            assert!(seen.insert(p.id), "{} twice", p.id);
            assert!(!p.markers.is_empty(), "{} has no marker", p.id);
        }
        for p in PROJECTS {
            for b in p.beats {
                assert!(seen.contains(b) && *b != p.id, "{} beats {b}", p.id);
            }
        }
    }

    #[test]
    fn templates_hold_a_placeholder() {
        for p in PROJECTS {
            for s in p.from {
                assert!(!s.templates.is_empty() && !s.files.is_empty(), "{}", p.id);
                for t in s.templates {
                    assert!(t.contains("{}"), "{}: {t}", p.id);
                }
            }
        }
    }

    #[test]
    fn pnpm_entry_reads_package_json_scripts() {
        let p = get("pnpm");
        assert_eq!(p.markers, [Marker::Name("pnpm-lock.yaml")]);
        assert_eq!(p.requires, ["package.json"]);
        assert_eq!(p.beats, ["npm"]);
        let s = p.from[0];
        assert_eq!(s.reader, Reader::JsonKeys);
        assert_eq!(s.files, ["package.json"]);
        assert_eq!(s.path, "scripts");
        assert_eq!(s.templates, ["pnpm {}", "pnpm run {}"]);
    }

    #[test]
    fn xcode_markers_are_extensions_and_flutter_needs_its_sdk() {
        assert_eq!(
            get("xcode").markers,
            [Marker::Ext(".xcodeproj"), Marker::Ext(".xcworkspace")]
        );
        let c = get("flutter").contains.unwrap();
        assert_eq!((c.file, c.text), ("pubspec.yaml", "sdk: flutter"));
        assert_eq!(
            get("android").requires_any,
            ["app/build.gradle", "app/build.gradle.kts"]
        );
    }
}
