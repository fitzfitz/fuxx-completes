//! The order history matches are offered in (`docs/design.md` §5.4): commands that ran
//! successfully in this folder first (more often first; no frequency boost for `cd`, `pushd` and
//! `open`), then the rest newest first, and commands that only ever failed last; the command
//! that followed the previous one in this session goes first within its tier. With no context
//! the order is M0's: session records newest first, then file entries newest first.

use crate::cd_target::command_word;
use crate::context::index::{ContextIndex, Tally};
use crate::context::Key;
use crate::history::History;
pub use crate::history::{Verdict, CHECK_BUDGET, MAX_SKIPPED};
use std::collections::{HashSet, VecDeque};
use std::path::Path;
use std::time::{Duration, Instant};

/// Pairs of consecutive `ran` reports kept in memory.
pub const MAX_PAIRS: usize = 10_000;

/// True for `cd`, `pushd` and `open`: they rank by where they ran, not by how often (the
/// user's decision, 2026-11-21).
pub fn no_frequency_boost(command: &str) -> bool {
    matches!(
        command_word(command).as_deref(),
        Some("cd" | "pushd" | "open")
    )
}

/// Which command followed which in this session (`ran` reports in order), in memory only.
#[derive(Debug, Default)]
pub struct Successors {
    pairs: VecDeque<(String, String)>,
}

impl Successors {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, prev: &str, next: &str) {
        if self.pairs.len() >= MAX_PAIRS {
            self.pairs.pop_front();
        }
        self.pairs.push_back((prev.to_owned(), next.to_owned()));
    }

    pub fn follows(&self, prev: &str, next: &str) -> bool {
        self.pairs.iter().any(|(p, n)| p == prev && n == next)
    }

    /// What followed `prev`, newest pair first.
    fn next_of<'s>(&'s self, prev: &'s str) -> impl Iterator<Item = &'s str> + 's {
        self.pairs
            .iter()
            .rev()
            .filter(move |(p, _)| p == prev)
            .map(|(_, n)| n.as_str())
    }
}

/// What the order is computed from, besides history.
#[derive(Debug, Clone, Copy)]
pub struct Ranking<'r> {
    /// The context records' tallies (`None`: no context).
    pub index: Option<&'r ContextIndex>,
    /// The request's folder; tallies count records in it exactly.
    pub cwd: &'r Path,
    /// The command just run (the adapter's `prev`); `None` when unknown.
    pub previous: Option<&'r str>,
    pub successors: Option<&'r Successors>,
}

impl<'r> Ranking<'r> {
    /// No context: the order is M0's.
    pub fn none(cwd: &'r Path) -> Self {
        Ranking {
            index: None,
            cwd,
            previous: None,
            successors: None,
        }
    }
}

/// History entries starting with a prefix, distinct, in the ranking order (`docs/design.md`
/// §5.4): tier 1 (ran successfully here: the command that followed the previous one first, then
/// the most runs here, then the newest run here, then recency); then the tier-2 commands that
/// followed the previous one; then the rest newest first; then those that only ever failed
/// (here, or anywhere when they never ran here). Lazy past the small tier-1 and follows sets.
pub struct Ranked<'a, 'r> {
    history: &'a History,
    prefix: String,
    ranking: Ranking<'r>,
    head: std::vec::IntoIter<&'a str>,
    seen: HashSet<&'a str>,
    walk: Box<dyn Iterator<Item = (&'a str, Option<Key>)> + 'a>,
    deferred: Option<Vec<&'a str>>,
    tail: std::vec::IntoIter<&'a str>,
}

impl<'a, 'r> Ranked<'a, 'r> {
    pub(crate) fn new(history: &'a History, prefix: &str, ranking: Ranking<'r>) -> Self {
        let follows: Vec<&'a str> = match (ranking.previous, ranking.successors) {
            (Some(p), Some(s)) => {
                let mut out: Vec<&'a str> = Vec::new();
                for next in s.next_of(p).filter(|n| n.starts_with(prefix)) {
                    if let Some(e) = history.find(next) {
                        if !out.contains(&e) {
                            out.push(e);
                        }
                    }
                }
                out
            }
            _ => Vec::new(),
        };
        let index = ranking.index.filter(|_| history.is_keyed());
        // Tier 1: ran successfully here.
        // (follows, runs here, newest run here, recency), then the text.
        let mut tier1: Vec<(TierOneRank, &'a str)> = Vec::new();
        if let Some(here) = index.and_then(|ix| ix.in_folder(ranking.cwd)) {
            for (&key, tally) in here {
                if tally.ok == 0 {
                    continue;
                }
                let Some(text) = history.text_for(key).filter(|t| t.starts_with(prefix)) else {
                    continue;
                };
                let freq = if no_frequency_boost(text) {
                    1
                } else {
                    tally.ok
                };
                let f = follows.contains(&text);
                tier1.push(((f, freq, tally.last_time, history.recency(key)), text));
            }
        }
        tier1.sort_by_key(|t| std::cmp::Reverse(t.0));
        let mut head: Vec<&'a str> = tier1.into_iter().map(|t| t.1).collect();
        // Then the tier-2 commands that followed the previous one, newest first.
        let mut tier2_follows: Vec<&'a str> = follows
            .into_iter()
            .filter(|t| !head.contains(t))
            .filter(|t| {
                let key = history.key_of(t);
                !key.is_some_and(|k| failed_only(index, ranking.cwd, k))
            })
            .collect();
        if history.is_keyed() {
            tier2_follows
                .sort_by_key(|t| std::cmp::Reverse(history.key_of(t).map(|k| history.recency(k))));
        }
        head.extend(tier2_follows);
        Ranked {
            history,
            prefix: prefix.to_owned(),
            ranking,
            head: head.into_iter(),
            seen: HashSet::new(),
            walk: Box::new(history.newest_first_keyed()),
            deferred: Some(Vec::new()),
            tail: Vec::new().into_iter(),
        }
    }
}

/// What orders tier 1, larger first: follows the previous command, runs here (1 for `cd`,
/// `pushd`, `open`), the newest run here, recency.
type TierOneRank = (bool, u32, i64, (bool, u64));

/// Tier 3: no success here but a failure here, or no record here and only failures elsewhere.
fn failed_only(index: Option<&ContextIndex>, cwd: &Path, key: Key) -> bool {
    let Some(ix) = index else {
        return false;
    };
    match ix.in_folder(cwd).and_then(|h| h.get(&key)) {
        Some(Tally { ok, failed, .. }) => *ok == 0 && *failed > 0,
        None => ix.anywhere(key).is_some_and(|t| t.ok == 0 && t.failed > 0),
    }
}

impl<'a> Iterator for Ranked<'a, '_> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if let Some(t) = self.head.next() {
            self.seen.insert(t);
            return Some(t);
        }
        if let Some(deferred) = &mut self.deferred {
            let index = self.ranking.index.filter(|_| self.history.is_keyed());
            for (text, key) in self.walk.by_ref() {
                if !text.starts_with(self.prefix.as_str()) || !self.seen.insert(text) {
                    continue;
                }
                if key.is_some_and(|k| failed_only(index, self.ranking.cwd, k)) {
                    deferred.push(text);
                    continue;
                }
                return Some(text);
            }
            self.tail = self.deferred.take().unwrap_or_default().into_iter();
        }
        self.tail.next()
    }
}

/// M0's walk over `candidates` in order, asking `judge` about each distinct one once: the
/// first `Best` or `Plain` one ends the walk (a `Plain` one gives way to an earlier held one);
/// the first `Hold` one is kept aside; `Skip` ones are passed over. After `MAX_SKIPPED` distinct
/// candidates passed over (skipped or held), or once `budget` has passed, the walk stops with
/// the held candidate; with none held, the bound gives `None` and the budget gives the next
/// candidate unchecked (`docs/design.md` §3, §5.4).
pub fn walk<'a>(
    candidates: impl Iterator<Item = &'a str>,
    budget: Duration,
    mut judge: impl FnMut(&str) -> Verdict,
) -> Option<&'a str> {
    let start = Instant::now();
    let mut passed: Vec<&str> = Vec::new();
    let mut held: Option<&str> = None;
    for entry in candidates {
        if passed.contains(&entry) {
            continue;
        }
        if start.elapsed() >= budget {
            return held.or(Some(entry));
        }
        match judge(entry) {
            Verdict::Best => return Some(entry),
            Verdict::Plain => return held.or(Some(entry)),
            Verdict::Hold => {
                held.get_or_insert(entry);
            }
            Verdict::Skip => {}
        }
        passed.push(entry);
        if passed.len() >= MAX_SKIPPED {
            return held;
        }
    }
    held
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::index::ContextIndex;
    use crate::context::record::Record;
    use crate::context::secret::Secret;
    use crate::history::History;
    use std::path::{Path, PathBuf};

    fn secret() -> Secret {
        Secret::from_bytes([9; 32])
    }

    /// History from `file` (oldest first) with the test secret, and records
    /// `(command, folder, status, time)`.
    fn setup(file: &[&str], recs: &[(&str, &str, i32, i64)]) -> (History, ContextIndex) {
        let s = secret();
        let mut h = History::new();
        h.replace_file_entries(file.iter().map(|e| e.to_string()).collect());
        h.set_secret(Some(&s));
        let mut ix = ContextIndex::default();
        for &(c, f, status, time) in recs {
            ix.add(&Record {
                key: s.key_of(c),
                folder: PathBuf::from(f),
                status,
                time,
            });
        }
        (h, ix)
    }

    fn at<'r>(ix: &'r ContextIndex, cwd: &'r str) -> Ranking<'r> {
        Ranking {
            index: Some(ix),
            cwd: Path::new(cwd),
            previous: None,
            successors: None,
        }
    }

    fn order<'a>(h: &'a History, prefix: &str, r: &Ranking) -> Vec<&'a str> {
        h.ranked_with_prefix(prefix, r).collect()
    }

    #[test]
    fn no_context_order_is_m0s() {
        let mut h = History::new();
        h.replace_file_entries(
            ["git a", "git b", "git a", "ls", "git c"]
                .iter()
                .map(|e| e.to_string())
                .collect(),
        );
        h.record_session("git s1".into());
        h.record_session("git b".into());
        let r = Ranking::none(Path::new("/p"));
        assert_eq!(order(&h, "git", &r), ["git b", "git s1", "git c", "git a"]);
        // The same with a secret but no records.
        h.set_secret(Some(&secret()));
        let ix = ContextIndex::default();
        assert_eq!(
            order(&h, "git", &at(&ix, "/p")),
            ["git b", "git s1", "git c", "git a"]
        );
    }

    #[test]
    fn ran_here_successfully_beats_newer_history() {
        let (h, ix) = setup(&["make test", "make build"], &[("make test", "/p", 0, 1)]);
        assert_eq!(
            order(&h, "make ", &at(&ix, "/p")),
            ["make test", "make build"]
        );
        assert_eq!(
            order(&h, "make ", &at(&ix, "/q")),
            ["make build", "make test"]
        );
    }

    #[test]
    fn frequency_here_orders_tier_one() {
        let recs = [
            ("git status", "/p", 0, 1),
            ("git status", "/p", 0, 2),
            ("git status", "/p", 0, 3),
            ("git stash", "/p", 0, 9),
        ];
        let (h, ix) = setup(&["git status", "git stash"], &recs);
        assert_eq!(
            order(&h, "git st", &at(&ix, "/p")),
            ["git status", "git stash"]
        );
        // A tie in frequency goes to the newest record here.
        let recs = [("git a", "/p", 0, 5), ("git b", "/p", 0, 7)];
        let (h, ix) = setup(&["git b", "git a"], &recs);
        assert_eq!(order(&h, "git", &at(&ix, "/p")), ["git b", "git a"]);
    }

    #[test]
    fn cd_pushd_open_get_no_frequency_boost() {
        let mut recs: Vec<(&str, &str, i32, i64)> =
            (0..9).map(|i| ("cd src", "/p", 0, i)).collect();
        recs.push(("cd lib", "/p", 0, 100));
        let (h, ix) = setup(&["cd lib", "cd src"], &recs);
        assert_eq!(order(&h, "cd ", &at(&ix, "/p")), ["cd lib", "cd src"]);
        let mut recs: Vec<(&str, &str, i32, i64)> =
            (0..9).map(|i| ("open a.txt", "/p", 0, i)).collect();
        recs.push(("open b.txt", "/p", 0, 100));
        let (h, ix) = setup(&["open b.txt", "open a.txt"], &recs);
        assert_eq!(
            order(&h, "open ", &at(&ix, "/p")),
            ["open b.txt", "open a.txt"]
        );
        // A command that isn't one of them does get the boost.
        let mut recs: Vec<(&str, &str, i32, i64)> = (0..9).map(|i| ("cdk a", "/p", 0, i)).collect();
        recs.push(("cdk b", "/p", 0, 100));
        let (h, ix) = setup(&["cdk b", "cdk a"], &recs);
        assert_eq!(order(&h, "cdk ", &at(&ix, "/p")), ["cdk a", "cdk b"]);
        assert!(no_frequency_boost("builtin cd x"));
        assert!(no_frequency_boost("FOO=1 pushd y"));
        assert!(no_frequency_boost("open ."));
        assert!(!no_frequency_boost("cdk deploy"));
        assert!(!no_frequency_boost("echo cd"));
        assert!(!no_frequency_boost("opener x"));
    }

    #[test]
    fn failed_only_ranks_last() {
        let (h, ix) = setup(&["git status", "gti status"], &[("gti status", "/p", 1, 5)]);
        assert_eq!(order(&h, "g", &at(&ix, "/p")), ["git status", "gti status"]);
        // Only ever failed elsewhere, no record here: below one with no records at all.
        let (h, ix) = setup(&["make a", "make b"], &[("make b", "/q", 2, 5)]);
        assert_eq!(order(&h, "make", &at(&ix, "/p")), ["make a", "make b"]);
        // Failed here once but succeeded here once: tier 1.
        let recs = [("make b", "/p", 2, 5), ("make b", "/p", 0, 6)];
        let (h, ix) = setup(&["make b", "make a"], &recs);
        assert_eq!(order(&h, "make", &at(&ix, "/p")), ["make b", "make a"]);
        // Succeeded elsewhere, failed here: failed only here counts (tier 3).
        let recs = [("make b", "/q", 0, 5), ("make b", "/p", 1, 6)];
        let (h, ix) = setup(&["make b", "make a"], &recs);
        assert_eq!(order(&h, "make", &at(&ix, "/p")), ["make a", "make b"]);
    }

    #[test]
    fn follows_previous_ranks_first_within_its_tier() {
        let (h, ix) = setup(&["git commit", "git checkout"], &[]);
        let mut succ = Successors::new();
        succ.record("git add .", "git commit");
        let mut r = at(&ix, "/p");
        r.successors = Some(&succ);
        assert_eq!(order(&h, "git c", &r), ["git checkout", "git commit"]);
        r.previous = Some("git add .");
        assert_eq!(order(&h, "git c", &r), ["git commit", "git checkout"]);
        assert!(succ.follows("git add .", "git commit"));
        assert!(!succ.follows("git commit", "git add ."));
        // Within tier 1, follows comes before frequency.
        let recs = [
            ("git x", "/p", 0, 1),
            ("git x", "/p", 0, 2),
            ("git commit", "/p", 0, 3),
        ];
        let (h, ix) = setup(&["git commit", "git x"], &recs);
        let mut r = at(&ix, "/p");
        r.successors = Some(&succ);
        r.previous = Some("git add .");
        assert_eq!(order(&h, "git", &r), ["git commit", "git x"]);
    }

    #[test]
    fn follows_works_without_a_secret() {
        let mut h = History::new();
        h.replace_file_entries(vec!["git commit".into(), "git checkout".into()]);
        let mut succ = Successors::new();
        succ.record("git add .", "git commit");
        let r = Ranking {
            index: None,
            cwd: Path::new("/p"),
            previous: Some("git add ."),
            successors: Some(&succ),
        };
        assert_eq!(order(&h, "git c", &r), ["git commit", "git checkout"]);
    }

    #[test]
    fn successors_are_bounded() {
        let mut succ = Successors::new();
        succ.record("first", "pair");
        for i in 0..MAX_PAIRS {
            succ.record("a", &format!("b{i}"));
        }
        assert!(!succ.follows("first", "pair"));
        assert!(succ.follows("a", &format!("b{}", MAX_PAIRS - 1)));
    }

    #[test]
    fn clearing_the_secret_gives_m0s_order_again() {
        let (mut h, ix) = setup(&["make test", "make build"], &[("make test", "/p", 0, 1)]);
        h.set_secret(None);
        assert_eq!(
            order(&h, "make ", &at(&ix, "/p")),
            ["make build", "make test"]
        );
    }

    #[test]
    fn session_records_get_keys_too() {
        let (mut h, ix) = setup(&["make build"], &[("make test", "/p", 0, 1)]);
        h.record_session("make test".into());
        h.record_session("make zzz".into());
        assert_eq!(
            order(&h, "make ", &at(&ix, "/p")),
            ["make test", "make zzz", "make build"]
        );
    }

    #[test]
    fn walk_is_m0s_walk() {
        let c = ["cd ..", "cd bad", "cd proj", "ls"];
        let judge = |s: &str| match s {
            "cd .." => Verdict::Hold,
            "cd bad" => Verdict::Skip,
            "cd proj" => Verdict::Best,
            _ => Verdict::Plain,
        };
        assert_eq!(
            walk(c.iter().copied(), CHECK_BUDGET, judge),
            Some("cd proj")
        );
        assert_eq!(
            walk(["ls", "cd proj"].into_iter(), CHECK_BUDGET, judge),
            Some("ls")
        );
    }

    #[test]
    fn a_dev_server_stopped_with_ctrl_c_is_not_failed_only() {
        let recs = [("npm run dev", "/p", 130, 5), ("npm run dev", "/p", 130, 6)];
        let (h, ix) = setup(&["npm run dev", "npm run build"], &recs);
        // Neutral: it ranks like a command with no records (newest first), not last.
        assert_eq!(
            order(&h, "npm run ", &at(&ix, "/p")),
            ["npm run build", "npm run dev"]
        );
        let (h, ix) = setup(&["npm run build", "npm run dev"], &recs);
        assert_eq!(
            order(&h, "npm run ", &at(&ix, "/p")),
            ["npm run dev", "npm run build"]
        );
    }
}
