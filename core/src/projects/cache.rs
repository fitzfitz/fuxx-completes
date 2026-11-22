//! The project cache (spec §4–§5.2): detections kept in memory per folder, filled by a worker
//! thread so an answer never waits on the disk.

use crate::projects::detect::{detect, Detection, Stamp};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Folders kept (least recently used dropped).
pub const MAX_FOLDERS: usize = 64;
/// How long one folder's detection may take before it stops with what it has.
pub const DETECT_BUDGET: Duration = Duration::from_millis(50);
/// A detection still running after this is given up (a hung disk), so other folders get theirs.
pub const STUCK_AFTER: Duration = Duration::from_secs(2);
/// Given-up detections still running at most; past this no new one starts.
const MAX_ABANDONED: usize = 2;
/// A detection cut short by its budget is tried again this many times before it is kept as is
/// (until its folder changes).
const MAX_TRIES: u8 = 3;

struct Entry {
    detection: Detection,
    used: u64,
    tries: u8,
}

/// Detections per folder, in memory only. `candidates` never touches more than a few `stat`s;
/// a missing or stale entry is (re)detected on one worker thread, and a stale one is served
/// meanwhile.
pub struct ProjectCache {
    entries: HashMap<PathBuf, Entry>,
    tick: u64,
    inflight: Option<(PathBuf, JoinHandle<Detection>, Instant)>,
    abandoned: Vec<JoinHandle<Detection>>,
    budget: Duration,
    stuck_after: Duration,
    /// Detections started (tests).
    starts: usize,
    /// Tests hold this to keep the worker from finishing.
    #[cfg(test)]
    gate: std::sync::Arc<std::sync::Mutex<()>>,
}

impl Default for ProjectCache {
    fn default() -> Self {
        ProjectCache {
            entries: HashMap::new(),
            tick: 0,
            inflight: None,
            abandoned: Vec::new(),
            budget: DETECT_BUDGET,
            stuck_after: STUCK_AFTER,
            starts: 0,
            #[cfg(test)]
            gate: Default::default(),
        }
    }
}

impl ProjectCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The candidates for `cwd` as last detected (empty while a never-seen folder is detected).
    pub fn candidates(&mut self, cwd: &Path, home: Option<&Path>) -> &[String] {
        self.collect(false);
        self.tick += 1;
        let fresh = self.entries.get(cwd).is_some_and(|e| {
            (e.detection.complete || e.tries >= MAX_TRIES)
                && e.detection
                    .stamps
                    .iter()
                    .all(|s| Stamp::of(&s.path).as_ref() == Some(s))
        });
        if !fresh {
            self.start(cwd, home);
        }
        match self.entries.get_mut(cwd) {
            Some(e) => {
                e.used = self.tick;
                &e.detection.candidates
            }
            None => &[],
        }
    }

    /// Waits for the detection in flight, if any, and keeps its result.
    pub fn wait_idle(&mut self) {
        self.collect(true);
    }

    fn collect(&mut self, wait: bool) {
        let done = self
            .inflight
            .as_ref()
            .is_some_and(|(_, h, _)| h.is_finished());
        if !(done || wait) {
            return;
        }
        let Some((path, handle, _)) = self.inflight.take() else {
            return;
        };
        let Ok(detection) = handle.join() else {
            return;
        };
        if !self.entries.contains_key(&path) && self.entries.len() >= MAX_FOLDERS {
            if let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.used)
                .map(|(p, _)| p.clone())
            {
                self.entries.remove(&oldest);
            }
        }
        self.tick += 1;
        let tries = match (&detection.complete, self.entries.get(&path)) {
            (true, _) => 0,
            (false, Some(old)) => old.tries.saturating_add(1),
            (false, None) => 1,
        };
        self.entries.insert(
            path,
            Entry {
                detection,
                used: self.tick,
                tries,
            },
        );
    }

    fn start(&mut self, cwd: &Path, home: Option<&Path>) {
        self.abandoned.retain(|h| !h.is_finished());
        if let Some((_, h, since)) = &self.inflight {
            let stuck = !h.is_finished() && since.elapsed() >= self.stuck_after;
            if !stuck || self.abandoned.len() >= MAX_ABANDONED {
                return;
            }
            if let Some((_, h, _)) = self.inflight.take() {
                self.abandoned.push(h);
            }
        }
        self.starts += 1;
        let (dir, home, budget) = (cwd.to_path_buf(), home.map(Path::to_path_buf), self.budget);
        #[cfg(test)]
        let gate = self.gate.clone();
        let handle = std::thread::spawn(move || {
            #[cfg(test)]
            let _held = gate.lock();
            detect(&dir, home.as_deref(), Instant::now() + budget)
        });
        self.inflight = Some((cwd.to_path_buf(), handle, Instant::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn repo() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = std::fs::canonicalize(d.path()).unwrap();
        std::fs::write(p.join(".git"), "").unwrap();
        (d, p)
    }

    fn filled(c: &mut ProjectCache, dir: &Path) -> Vec<String> {
        c.candidates(dir, None);
        c.wait_idle();
        c.candidates(dir, None).to_vec()
    }

    /// Writes `text` so the file's (mtime, size) surely differ from before.
    fn rewrite(p: &Path, text: &str) {
        std::thread::sleep(Duration::from_millis(5));
        std::fs::write(p, text).unwrap();
    }

    #[test]
    fn cold_folder_answers_empty_then_fills() {
        let (_d, r) = repo();
        std::fs::write(r.join("Cargo.toml"), "").unwrap();
        let mut c = ProjectCache::new();
        assert!(c.candidates(&r, None).is_empty());
        c.wait_idle();
        assert_eq!(&c.candidates(&r, None)[..2], ["cargo build", "cargo test"]);
    }

    #[test]
    fn fresh_entry_is_served_without_detecting_again() {
        let (_d, r) = repo();
        std::fs::write(r.join("Cargo.toml"), "").unwrap();
        let mut c = ProjectCache::new();
        filled(&mut c, &r);
        for _ in 0..10 {
            c.candidates(&r, None);
        }
        c.wait_idle();
        assert_eq!(c.starts, 1);
    }

    #[test]
    fn a_changed_script_is_picked_up() {
        let (_d, r) = repo();
        let pkg = r.join("package.json");
        std::fs::write(&pkg, r#"{"scripts":{"old":"x"}}"#).unwrap();
        let mut c = ProjectCache::new();
        assert!(filled(&mut c, &r).contains(&"npm run old".to_string()));
        rewrite(&pkg, r#"{"scripts":{"newer":"x"}}"#);
        let now = filled(&mut c, &r);
        assert!(now.contains(&"npm run newer".to_string()), "{now:?}");
        assert!(!now.contains(&"npm run old".to_string()));
    }

    #[test]
    fn a_deleted_makefile_drops_its_targets() {
        let (_d, r) = repo();
        std::fs::write(r.join("Makefile"), "fx:\n").unwrap();
        let mut c = ProjectCache::new();
        assert!(filled(&mut c, &r).contains(&"make fx".to_string()));
        std::thread::sleep(Duration::from_millis(5));
        std::fs::remove_file(r.join("Makefile")).unwrap();
        assert!(filled(&mut c, &r).is_empty());
    }

    #[test]
    fn stale_entry_is_served_while_redetecting() {
        let (_d, r) = repo();
        let pkg = r.join("package.json");
        std::fs::write(&pkg, r#"{"scripts":{"old":"x"}}"#).unwrap();
        let mut c = ProjectCache::new();
        filled(&mut c, &r);
        rewrite(&pkg, r#"{"scripts":{"newer":"x"}}"#);
        let gate = c.gate.clone();
        let held = gate.lock().unwrap();
        assert!(c.candidates(&r, None).contains(&"npm run old".to_string()));
        drop(held);
        c.wait_idle();
        assert!(c
            .candidates(&r, None)
            .contains(&"npm run newer".to_string()));
    }

    #[test]
    fn lru_keeps_64_folders() {
        let (_d, r) = repo();
        let dirs: Vec<PathBuf> = (0..=MAX_FOLDERS)
            .map(|i| {
                let d = r.join(format!("p{i}"));
                std::fs::create_dir_all(&d).unwrap();
                std::fs::write(d.join("Cargo.toml"), "").unwrap();
                d
            })
            .collect();
        let mut c = ProjectCache::new();
        for d in &dirs {
            filled(&mut c, d);
        }
        assert_eq!(c.entries.len(), MAX_FOLDERS);
        assert!(
            c.candidates(&dirs[0], None).is_empty(),
            "the oldest was dropped"
        );
        c.wait_idle();
        assert!(!c.candidates(&dirs[MAX_FOLDERS], None).is_empty());
    }

    #[test]
    fn one_detection_in_flight() {
        let (_d, r) = repo();
        let (a, b) = (r.join("a"), r.join("b"));
        for d in [&a, &b] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("Cargo.toml"), "").unwrap();
        }
        let mut c = ProjectCache::new();
        let gate = c.gate.clone();
        let held = gate.lock().unwrap();
        assert!(c.candidates(&a, None).is_empty());
        assert!(c.candidates(&b, None).is_empty());
        assert_eq!(c.starts, 1, "b waits for a");
        drop(held);
        c.wait_idle();
        assert!(!c.candidates(&a, None).is_empty());
        assert!(c.candidates(&b, None).is_empty());
        c.wait_idle();
        assert!(!c.candidates(&b, None).is_empty());
    }

    #[test]
    fn candidates_call_is_fast_when_warm() {
        let (_d, r) = repo();
        std::fs::write(r.join("package.json"), r#"{"scripts":{"a":"x"}}"#).unwrap();
        let mut c = ProjectCache::new();
        filled(&mut c, &r);
        let start = Instant::now();
        for _ in 0..1000 {
            assert!(!c.candidates(&r, None).is_empty());
        }
        assert!(
            start.elapsed() < Duration::from_millis(50),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn a_cut_short_detection_is_retried() {
        let (_d, r) = repo();
        std::fs::write(r.join("Cargo.toml"), "").unwrap();
        let mut c = ProjectCache::new();
        c.budget = Duration::ZERO;
        assert!(filled(&mut c, &r).is_empty());
        c.budget = DETECT_BUDGET * 100;
        c.wait_idle(); // the zero-budget retry started by the last call
        assert!(!filled(&mut c, &r).is_empty(), "never retried");
    }

    #[test]
    fn a_stuck_detection_is_given_up() {
        let (_d, r) = repo();
        let (a, b) = (r.join("a"), r.join("b"));
        for d in [&a, &b] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("Cargo.toml"), "").unwrap();
        }
        let mut c = ProjectCache::new();
        c.stuck_after = Duration::from_millis(20);
        let gate = c.gate.clone();
        let held = gate.lock().unwrap();
        c.candidates(&a, None);
        std::thread::sleep(Duration::from_millis(40));
        c.candidates(&b, None);
        assert_eq!(c.starts, 2, "a stuck detection blocks every later folder");
        drop(held);
        c.wait_idle();
        assert!(!c.candidates(&b, None).is_empty());
    }
}
