//! Detecting the projects around a folder (spec §5.2): walk up, one listing per folder, match
//! the mapping table, read small files, and turn what they hold into candidate commands.

use crate::projects::readers::{
    file_names, json_keys, just_recipes, make_targets, read_small, toml_keys, yaml_keys,
};
use crate::projects::{Marker, ProjectType, Reader, Source, PROJECTS};
use std::collections::HashSet;
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
/// the stamps that keep it fresh, the project ids found (nearest first), and whether the
/// detection ran to the end (`false`: the deadline cut it short, so it should be tried again).
#[derive(Debug, Clone, Default)]
pub struct Detection {
    pub candidates: Vec<String>,
    pub stamps: Vec<Stamp>,
    pub ids: Vec<&'static str>,
    pub complete: bool,
    /// Where each candidate came from, one per candidate (for `why`, M1d).
    pub origins: Vec<Origin>,
    /// The folders `Origin::folder` points into.
    pub folders: Vec<PathBuf>,
}

/// Where a candidate came from: the project type, the folder (an index into
/// `Detection::folders`) and the file a reader took it from (`None`: a fixed command).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Origin {
    pub id: &'static str,
    pub folder: usize,
    pub file: Option<&'static str>,
}

/// One folder on the walk: its path, its listing, and the types matched there.
type Scanned = (PathBuf, Vec<String>, Vec<&'static ProjectType>);

/// Walks up from `cwd`: at most `MAX_LEVELS` folders, never `home` or `/`, stopping after a
/// folder holding `.git`. Stops with what it has once `deadline` passes (then `complete` is
/// false), or at a folder it can't list. Never runs anything.
pub fn detect(cwd: &Path, home: Option<&Path>, deadline: Instant) -> Detection {
    let mut out = Detection::default();
    let mut cut = false;
    let mut walked: Vec<Scanned> = Vec::new();
    let mut folder = Some(cwd);
    for _ in 0..MAX_LEVELS {
        let Some(dir) = folder else { break };
        if Some(dir) == home || dir.parent().is_none() {
            break;
        }
        if Instant::now() >= deadline {
            cut = true;
            break;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            break;
        };
        let names: Vec<String> = entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .collect();
        out.stamps.extend(Stamp::of(dir));
        let mut found: Vec<&'static ProjectType> = PROJECTS
            .iter()
            .filter(|t| matches(t, dir, &names, &mut out))
            .collect();
        let beaten: Vec<&str> = found.iter().flat_map(|t| t.beats.iter().copied()).collect();
        found.retain(|t| !beaten.contains(&t.id));
        let stop = names.iter().any(|n| n == ".git");
        walked.push((dir.to_path_buf(), names, found));
        if stop {
            break;
        }
        folder = dir.parent();
    }
    // A type found further up takes the place of the types it `beats_below` in nearer folders
    // (the packages of a pnpm, yarn or bun workspace).
    for up in 1..walked.len() {
        let above: Vec<&'static ProjectType> = walked[up].2.clone();
        for t in above.iter().filter(|t| !t.beats_below.is_empty()) {
            for near in &mut walked[..up] {
                let had = near.2.len();
                near.2.retain(|x| !t.beats_below.contains(&x.id));
                if near.2.len() != had && !near.2.iter().any(|x| x.id == t.id) {
                    near.2.push(t);
                }
            }
        }
    }
    let mut seen = HashSet::new();
    'walk: for (dir, names, found) in &walked {
        out.folders.push(dir.clone());
        let folder = out.folders.len() - 1;
        for t in found {
            out.ids.push(t.id);
            let fixed = Origin {
                id: t.id,
                folder,
                file: None,
            };
            for c in t.commands {
                push(&mut out, &mut seen, c.to_string(), fixed);
            }
            for s in t.from {
                if Instant::now() >= deadline {
                    cut = true;
                    break 'walk;
                }
                let (items, file) = source_names(s, dir, names, &mut out);
                let from = Origin {
                    id: t.id,
                    folder,
                    file,
                };
                for item in items {
                    for tpl in s.templates {
                        push(&mut out, &mut seen, tpl.replace("{}", &item), from);
                    }
                }
            }
        }
    }
    out.complete = !cut;
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

/// The names one source pulls out of a folder, and the file (or pattern) they came from.
fn source_names(
    s: &Source,
    dir: &Path,
    names: &[String],
    out: &mut Detection,
) -> (Vec<String>, Option<&'static str>) {
    if s.reader == Reader::FileNames {
        let found = s
            .files
            .iter()
            .flat_map(|f| file_names(names, f.trim_start_matches('*')))
            .collect();
        return (found, s.files.first().copied());
    }
    let Some(file) = s.files.iter().find(|f| exists(dir, names, f)) else {
        return (Vec::new(), None);
    };
    let Some(text) = read(dir, file, out) else {
        return (Vec::new(), None);
    };
    let found = match s.reader {
        Reader::JsonKeys => json_keys(&text, s.path),
        Reader::TomlKeys => toml_keys(&text, s.path),
        Reader::YamlKeys => yaml_keys(&text, s.path),
        Reader::MakeTargets => make_targets(&text),
        Reader::JustRecipes => just_recipes(&text),
        Reader::FileNames => Vec::new(),
    };
    (found, Some(file))
}

fn push(out: &mut Detection, seen: &mut HashSet<String>, c: String, from: Origin) {
    if seen.insert(c.clone()) {
        out.candidates.push(c);
        out.origins.push(from);
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

    #[test]
    fn a_pnpm_workspace_package_uses_pnpm() {
        let (_d, r) = repo();
        write(&r.join("package.json"), r#"{"scripts":{"lint":"x"}}"#);
        write(&r.join("pnpm-lock.yaml"), "");
        let app = r.join("packages/app");
        write(&app.join("package.json"), r#"{"scripts":{"dev":"x"}}"#);
        let d = detect(&app, None, later());
        assert_eq!(d.ids, ["pnpm", "pnpm"]);
        for want in ["pnpm install", "pnpm dev", "pnpm run dev", "pnpm lint"] {
            assert!(
                d.candidates.contains(&want.to_string()),
                "{want}: {:?}",
                d.candidates
            );
        }
        assert!(
            !d.candidates.iter().any(|c| c.starts_with("npm ")),
            "{:?}",
            d.candidates
        );
    }

    #[test]
    fn a_cut_short_detection_says_so() {
        let (_d, r) = repo();
        write(&r.join("Cargo.toml"), "");
        assert!(!detect(&r, None, Instant::now()).complete);
        assert!(detect(&r, None, later()).complete);
    }

    #[test]
    fn huge_files_are_capped_and_quick() {
        let (_d, r) = repo();
        let keys: Vec<String> = (0..20_000).map(|i| format!("\"k{i}\":1")).collect();
        write(
            &r.join("package.json"),
            &format!("{{\"scripts\":{{{}}}}}", keys.join(",")),
        );
        let start = Instant::now();
        let d = detect(&r, None, later());
        assert!(
            start.elapsed() < Duration::from_millis(300),
            "{:?}",
            start.elapsed()
        );
        let runs = d
            .candidates
            .iter()
            .filter(|c| c.starts_with("npm run "))
            .count();
        assert_eq!(runs, crate::projects::readers::MAX_NAMES);
    }
}
