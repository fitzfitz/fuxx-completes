//! Detecting the projects around a folder (spec §5.2): walk up, one listing per folder, match
//! the mapping table, read small files, and turn what they hold into candidate commands.

use crate::projects::readers::{
    file_names, json_keys, just_recipes, make_targets, read_small, toml_keys, yaml_keys,
};
use crate::projects::{Marker, ProjectType, Reader, PROJECTS};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// How many folders the walk looks at, the cwd included (spec §5.2).
pub const MAX_LEVELS: usize = 8;

/// A folder listed or a file read, as it was: if any changes, the detection is stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamp {
    pub path: PathBuf,
    pub mtime: (i64, i64),
    pub len: u64,
}

impl Stamp {
    pub fn of(path: &Path) -> Option<Stamp> {
        let m = std::fs::metadata(path).ok()?;
        Some(Stamp {
            path: path.to_path_buf(),
            mtime: (m.mtime(), m.mtime_nsec()),
            len: m.len(),
        })
    }
}

/// What a folder's projects offer: candidate commands in order (nearest project first),
/// the stamps that keep it fresh, and the project ids found (nearest first).
#[derive(Debug, Clone, Default)]
pub struct Detection {
    pub candidates: Vec<String>,
    pub stamps: Vec<Stamp>,
    pub ids: Vec<&'static str>,
}

/// Walks up from `cwd`: at most `MAX_LEVELS` folders, never `home` or `/`, stopping after a
/// folder holding `.git`. Stops with what it has once `deadline` passes, or at a folder it can't
/// list. Never runs anything.
pub fn detect(cwd: &Path, home: Option<&Path>, deadline: Instant) -> Detection {
    let mut out = Detection::default();
    let mut folder = Some(cwd);
    for _ in 0..MAX_LEVELS {
        let Some(dir) = folder else { break };
        if Some(dir) == home || dir.parent().is_none() || Instant::now() >= deadline {
            break;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            break;
        };
        let names: Vec<String> = entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .collect();
        out.stamps.extend(Stamp::of(dir));
        scan(dir, &names, deadline, &mut out);
        if names.iter().any(|n| n == ".git") {
            break;
        }
        folder = dir.parent();
    }
    out
}

/// Present in the listing, or (a path with `/`) on disk.
fn exists(dir: &Path, names: &[String], rel: &str) -> bool {
    if rel.contains('/') {
        dir.join(rel).exists()
    } else {
        names.iter().any(|n| n == rel)
    }
}

fn read(dir: &Path, rel: &str, out: &mut Detection) -> Option<String> {
    let path = dir.join(rel);
    let text = read_small(&path)?;
    out.stamps.extend(Stamp::of(&path));
    Some(text)
}

fn matches(t: &ProjectType, dir: &Path, names: &[String], out: &mut Detection) -> bool {
    let marked = t.markers.iter().any(|m| match m {
        Marker::Name(n) => names.iter().any(|x| x == n),
        Marker::Ext(e) => names.iter().any(|x| x.len() > e.len() && x.ends_with(e)),
    });
    marked
        && t.requires.iter().all(|r| exists(dir, names, r))
        && (t.requires_any.is_empty() || t.requires_any.iter().any(|r| exists(dir, names, r)))
        && t.contains
            .is_none_or(|c| read(dir, c.file, out).is_some_and(|text| text.contains(c.text)))
}

/// One folder: the types that match (minus those another match beats), in table order, and
/// their candidates.
fn scan(dir: &Path, names: &[String], deadline: Instant, out: &mut Detection) {
    let mut found: Vec<&'static ProjectType> = Vec::new();
    for t in PROJECTS {
        if matches(t, dir, names, out) {
            found.push(t);
        }
    }
    let beaten: Vec<&str> = found.iter().flat_map(|t| t.beats.iter().copied()).collect();
    found.retain(|t| !beaten.contains(&t.id));
    for t in found {
        out.ids.push(t.id);
        for c in t.commands {
            push(out, c.to_string());
        }
        for s in t.from {
            if Instant::now() >= deadline {
                return;
            }
            let items = match s.reader {
                Reader::FileNames => s
                    .files
                    .iter()
                    .flat_map(|f| file_names(names, f.trim_start_matches('*')))
                    .collect(),
                reader => {
                    let Some(file) = s.files.iter().find(|f| exists(dir, names, f)) else {
                        continue;
                    };
                    let Some(text) = read(dir, file, out) else {
                        continue;
                    };
                    match reader {
                        Reader::JsonKeys => json_keys(&text, s.path),
                        Reader::TomlKeys => toml_keys(&text, s.path),
                        Reader::YamlKeys => yaml_keys(&text, s.path),
                        Reader::MakeTargets => make_targets(&text),
                        Reader::JustRecipes => just_recipes(&text),
                        Reader::FileNames => unreachable!(),
                    }
                }
            };
            for item in items {
                for tpl in s.templates {
                    push(out, tpl.replace("{}", &item));
                }
            }
        }
    }
}

fn push(out: &mut Detection, c: String) {
    if !out.candidates.contains(&c) {
        out.candidates.push(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn later() -> Instant {
        Instant::now() + Duration::from_secs(5)
    }

    /// A temp folder holding a `.git` file (the walk stops there), canonical path.
    fn repo() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = std::fs::canonicalize(d.path()).unwrap();
        std::fs::write(p.join(".git"), "").unwrap();
        (d, p)
    }

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn cargo_project_offers_cargo_commands() {
        let (_d, r) = repo();
        write(&r.join("Cargo.toml"), "[package]\nname = \"x\"\n");
        let d = detect(&r, None, later());
        assert_eq!(d.ids, ["cargo"]);
        assert_eq!(&d.candidates[..2], ["cargo build", "cargo test"]);
    }

    #[test]
    fn pnpm_beats_npm() {
        let (_d, r) = repo();
        write(&r.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#);
        write(&r.join("pnpm-lock.yaml"), "");
        let d = detect(&r, None, later());
        assert_eq!(d.ids, ["pnpm"]);
        assert_eq!(d.candidates, ["pnpm install", "pnpm dev", "pnpm run dev"]);
    }

    #[test]
    fn nearest_project_first_in_a_monorepo() {
        let (_d, r) = repo();
        write(&r.join("package.json"), r#"{"scripts":{"lint":"x"}}"#);
        let app = r.join("packages/app");
        write(&app.join("package.json"), r#"{"scripts":{"dev":"x"}}"#);
        let d = detect(&app, None, later());
        assert_eq!(d.ids, ["npm", "npm"]);
        let pos = |c: &str| d.candidates.iter().position(|x| x == c).unwrap();
        assert!(pos("npm run dev") < pos("npm run lint"));
        // Duplicates (npm install twice) are dropped.
        assert_eq!(
            d.candidates.iter().filter(|c| *c == "npm install").count(),
            1
        );
    }

    #[test]
    fn walk_stops_after_git_and_before_home() {
        let d0 = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(d0.path()).unwrap();
        write(&home.join("package.json"), r#"{"scripts":{"home":"x"}}"#);
        write(&home.join("outer/Makefile"), "outer:\n");
        let repo = home.join("outer/repo");
        write(&repo.join(".git"), "");
        write(&repo.join("Cargo.toml"), "");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        let d = detect(&repo.join("src"), Some(&home), later());
        assert_eq!(d.ids, ["cargo"]);
        assert!(detect(&home, Some(&home), later()).candidates.is_empty());
        let d = detect(&home.join("outer"), Some(&home), later());
        assert_eq!(d.ids, ["make"]);
        assert!(!d.candidates.iter().any(|c| c.contains("home")));
    }

    #[test]
    fn walk_is_at_most_8_levels() {
        let (_d, r) = repo();
        write(&r.join("package.json"), r#"{"scripts":{"top":"x"}}"#);
        let mut deep = r.clone();
        for i in 1..=8 {
            deep = deep.join(format!("l{i}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        assert!(
            detect(&deep, None, later()).ids.is_empty(),
            "9 levels up is too far"
        );
        assert_eq!(detect(deep.parent().unwrap(), None, later()).ids, ["npm"]);
    }

    #[test]
    fn deadline_in_the_past_gives_nothing_quickly() {
        let (_d, r) = repo();
        write(&r.join("Cargo.toml"), "");
        let start = Instant::now();
        let d = detect(&r, None, Instant::now());
        assert!(d.candidates.is_empty() && d.ids.is_empty());
        assert!(start.elapsed() < Duration::from_millis(20));
    }

    #[test]
    fn unreadable_parent_stops_quietly() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, r) = repo();
        let locked = r.join("locked");
        std::fs::create_dir_all(locked.join("inner")).unwrap();
        write(&locked.join("inner/Cargo.toml"), "");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let d = detect(&locked.join("inner"), None, later());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(d.ids.is_empty());
    }

    #[test]
    fn hostile_names_and_files() {
        let (_d, r) = repo();
        write(
            &r.join("package.json"),
            r#"{"scripts":{"rm -rf ~;":"x","ok":"y","a\u001b[2J":"z"}}"#,
        );
        let made = std::process::Command::new("mkfifo")
            .arg(r.join("Makefile"))
            .status()
            .unwrap();
        assert!(made.success());
        let start = Instant::now();
        let d = detect(&r, None, later());
        assert!(start.elapsed() < Duration::from_millis(200));
        assert!(d.candidates.contains(&"npm run ok".to_string()));
        assert!(!d
            .candidates
            .iter()
            .any(|c| c.contains("rm") || c.contains('\u{1b}')));
        assert!(
            d.candidates.contains(&"make".to_string()),
            "the Makefile marker still counts"
        );
    }

    #[test]
    fn stamps_cover_listed_folders_and_read_files() {
        let (_d, r) = repo();
        let sub = r.join("sub");
        write(&sub.join("justfile"), "fx:\n");
        let d = detect(&sub, None, later());
        let paths: Vec<&Path> = d.stamps.iter().map(|s| s.path.as_path()).collect();
        assert!(paths.contains(&sub.as_path()));
        assert!(paths.contains(&r.as_path()));
        assert!(paths.contains(&sub.join("justfile").as_path()));
        assert!(d.candidates.contains(&"just fx".to_string()));
    }
}
