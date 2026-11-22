//! What the context records say, tallied in memory: per folder and over every folder.

use crate::context::record::Record;
use crate::context::Key;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// How a command did: successful and failed runs, and the newest run's time.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tally {
    pub ok: u32,
    pub failed: u32,
    pub last_time: i64,
}

/// Exit statuses that count as neither a success nor a failure (the user's decision,
/// 2026-11-21): 130, stopped with ⌃C (a dev server, `tail -f`), and 143, ended with SIGTERM.
pub const NEUTRAL_STATUSES: [i32; 2] = [130, 143];

impl Tally {
    fn add(&mut self, r: &Record) {
        if r.status == 0 {
            self.ok = self.ok.saturating_add(1);
        } else if !NEUTRAL_STATUSES.contains(&r.status) {
            self.failed = self.failed.saturating_add(1);
        }
        self.last_time = self.last_time.max(r.time);
    }
}

/// Every record's tally, by folder (the folder exactly, byte-equal) and over all folders.
#[derive(Default, Clone, Debug)]
pub struct ContextIndex {
    folders: HashMap<PathBuf, HashMap<Key, Tally>>,
    all: HashMap<Key, Tally>,
    records: u64,
}

impl ContextIndex {
    pub fn add(&mut self, r: &Record) {
        self.folders
            .entry(r.folder.clone())
            .or_default()
            .entry(r.key)
            .or_default()
            .add(r);
        self.all.entry(r.key).or_default().add(r);
        self.records += 1;
    }

    /// Every folder with records, and its tallies.
    pub fn folders(&self) -> impl Iterator<Item = (&Path, &HashMap<Key, Tally>)> {
        self.folders.iter().map(|(f, t)| (f.as_path(), t))
    }

    /// How many records were added (a cheap way to tell whether the index changed).
    pub fn records(&self) -> u64 {
        self.records
    }

    /// The tallies of the commands that ran in `folder`.
    pub fn in_folder(&self, folder: &Path) -> Option<&HashMap<Key, Tally>> {
        self.folders.get(folder)
    }

    /// The tally of `key` over every folder.
    pub fn anywhere(&self, key: Key) -> Option<Tally> {
        self.all.get(&key).copied()
    }

    pub fn is_empty(&self) -> bool {
        self.all.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(key: u128, folder: &str, status: i32, time: i64) -> Record {
        Record {
            key: Key(key),
            folder: PathBuf::from(folder),
            status,
            time,
        }
    }

    #[test]
    fn tallies_split_by_folder_and_sum_everywhere() {
        let mut ix = ContextIndex::default();
        assert!(ix.is_empty());
        ix.add(&rec(1, "/p", 0, 10));
        ix.add(&rec(1, "/p", 2, 30));
        ix.add(&rec(1, "/q", 0, 20));
        ix.add(&rec(2, "/q", 1, 5));
        assert!(!ix.is_empty());
        let p = ix.in_folder(Path::new("/p")).unwrap();
        assert_eq!(
            p[&Key(1)],
            Tally {
                ok: 1,
                failed: 1,
                last_time: 30
            }
        );
        assert!(!p.contains_key(&Key(2)));
        assert_eq!(
            ix.anywhere(Key(1)),
            Some(Tally {
                ok: 2,
                failed: 1,
                last_time: 30
            })
        );
        assert_eq!(
            ix.anywhere(Key(2)),
            Some(Tally {
                ok: 0,
                failed: 1,
                last_time: 5
            })
        );
        assert_eq!(ix.anywhere(Key(3)), None);
        assert!(ix.in_folder(Path::new("/r")).is_none());
    }

    /// The user's decision (2026-11-21): a command stopped with ⌃C (130) or killed (143) is
    /// neither a success nor a failure; the record still says it ran here, and when.
    #[test]
    fn interrupted_or_killed_is_neutral() {
        let mut ix = ContextIndex::default();
        ix.add(&rec(1, "/p", 130, 7));
        ix.add(&rec(1, "/p", 143, 9));
        assert_eq!(
            ix.anywhere(Key(1)),
            Some(Tally {
                ok: 0,
                failed: 0,
                last_time: 9
            })
        );
        ix.add(&rec(1, "/p", 129, 10));
        assert_eq!(
            ix.anywhere(Key(1)).unwrap().failed,
            1,
            "other signals still count as failed"
        );
    }
}
