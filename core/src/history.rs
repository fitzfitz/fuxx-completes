//! The in-memory history index: entries read from the shell's history file plus the commands
//! the adapter reported this session. With a secret (the context file's, §5.7) every entry also
//! has its key, so context records can be matched to the text they came from.

use crate::context::secret::Secret;
use crate::context::Key;
use crate::rank::{walk, Ranked, Ranking};
use std::collections::{HashMap, VecDeque};
use std::str::FromStr;
use std::time::Duration;

/// How many distinct candidates `latest_with_prefix_except` may reject before it gives up.
pub const MAX_SKIPPED: usize = 50;

/// How long `latest_with_prefix_except` may spend checking candidates.
pub const CHECK_BUDGET: Duration = Duration::from_millis(50);

/// What `best_with_prefix` does with a candidate (`docs/design.md` §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Shown at once, above any held candidate (a `cd` down into the current folder).
    Best,
    /// Ends the walk: shown unless a newer candidate is held, which is shown instead (any other
    /// command, so its place in the order never changes).
    Plain,
    /// Kept aside: the newest held one is shown only when no `Best` candidate turns up within
    /// the walk's bounds (a `cd` that works from anywhere).
    Hold,
    /// Never shown (a `cd` to a folder that isn't there).
    Skip,
}

/// The shell history file formats the core can read: zsh (M0) and bash (M2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFormat {
    Zsh,
    Bash,
}

impl FromStr for HistoryFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "zsh" => Ok(HistoryFormat::Zsh),
            "bash" => Ok(HistoryFormat::Bash),
            _ => Err(()),
        }
    }
}

/// Where a key's text is: its newest file entry, and how many session records hold it (with
/// the sequence number of the newest).
#[derive(Debug, Default, Clone, Copy)]
struct Slot {
    file_newest: Option<usize>,
    session_count: u32,
    session_seq: u64,
}

/// Past commands, oldest first. `file` holds what the shell wrote to its history file;
/// `session` holds commands reported by the adapter that the file doesn't hold yet.
#[derive(Debug, Default)]
pub struct History {
    file: Vec<String>,
    session: Vec<String>,
    secret: Option<Secret>,
    /// With a secret: one key per file entry and per session record, in the same order.
    file_keys: Vec<Key>,
    session_keys: Vec<Key>,
    slots: HashMap<Key, Slot>,
    next_seq: u64,
    /// File entries read that no session record matched yet, with the `ran` count when they
    /// were read: a `ran` equal to one read since the `ran` before the previous one is already
    /// in the file (`SHARE_HISTORY`, `INC_APPEND_HISTORY`; design §5.8). Older ones came from
    /// other shells and never absorb a report.
    unmatched: VecDeque<(String, u64)>,
    /// How many `ran` reports were noted.
    rans: u64,
}

/// How many unmatched file entries are remembered for `note_ran`.
const MAX_UNMATCHED: usize = 1000;

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Most recent entry starting with `prefix`: session records newest-first, then file
    /// entries newest-first. Plain byte-prefix match, case-sensitive.
    pub fn latest_with_prefix(&self, prefix: &str) -> Option<&str> {
        self.newest_first().find(|entry| entry.starts_with(prefix))
    }

    /// Like `latest_with_prefix`, but walks past candidates that `skip` rejects. Each distinct
    /// rejected candidate counts once; after `MAX_SKIPPED` of them the walk stops with `None`.
    /// Once `CHECK_BUDGET` has passed, the next candidate is returned unchecked (a slow folder
    /// keeps a suggestion rather than miss the latency budget, `docs/design.md` §3, §5.4).
    pub fn latest_with_prefix_except(
        &self,
        prefix: &str,
        skip: impl FnMut(&str) -> bool,
    ) -> Option<&str> {
        self.latest_with_prefix_within(prefix, CHECK_BUDGET, skip)
    }

    /// `latest_with_prefix_except` with its time budget given (tests use a slow `skip`).
    pub fn latest_with_prefix_within(
        &self,
        prefix: &str,
        budget: Duration,
        mut skip: impl FnMut(&str) -> bool,
    ) -> Option<&str> {
        self.best_with_prefix_within(prefix, budget, |c| {
            if skip(c) {
                Verdict::Skip
            } else {
                Verdict::Plain
            }
        })
    }

    /// The best entry starting with `prefix` in recency order (session records, then file
    /// entries, newest first); see `rank::walk` for what `judge` decides.
    pub fn best_with_prefix(
        &self,
        prefix: &str,
        judge: impl FnMut(&str) -> Verdict,
    ) -> Option<&str> {
        self.best_with_prefix_within(prefix, CHECK_BUDGET, judge)
    }

    /// `best_with_prefix` with its time budget given (tests use a slow `judge`).
    pub fn best_with_prefix_within(
        &self,
        prefix: &str,
        budget: Duration,
        judge: impl FnMut(&str) -> Verdict,
    ) -> Option<&str> {
        walk(
            self.newest_first().filter(|e| e.starts_with(prefix)),
            budget,
            judge,
        )
    }

    /// Distinct entries starting with `prefix`, in the ranking order of `docs/design.md` §5.4.
    pub fn ranked_with_prefix<'a, 'r>(
        &'a self,
        prefix: &str,
        ranking: &Ranking<'r>,
    ) -> Ranked<'a, 'r> {
        Ranked::new(self, prefix, *ranking)
    }

    /// This folder's history for `prefix` (see `rank::here_order`).
    pub fn ranked_here<'a>(&'a self, prefix: &str, ranking: &Ranking) -> Vec<&'a str> {
        crate::rank::here_order(self, prefix, ranking)
    }

    /// This repo's history for `prefix` (see `rank::repo_order`).
    pub fn ranked_repo<'a>(&'a self, prefix: &str, ranking: &Ranking) -> Vec<&'a str> {
        crate::rank::repo_order(self, prefix, ranking)
    }

    /// Gives every entry its key under `secret`, or drops all keys (`None`).
    pub fn set_secret(&mut self, secret: Option<&Secret>) {
        self.secret = secret.cloned();
        self.rekey();
    }

    pub fn record_session(&mut self, command: String) {
        if let Some(s) = &self.secret {
            let key = s.key_of(&command);
            self.session_keys.push(key);
            self.next_seq += 1;
            let slot = self.slots.entry(key).or_default();
            slot.session_count += 1;
            slot.session_seq = self.next_seq;
        }
        self.session.push(command);
    }

    /// A command the adapter reported. If the file already gained it (a file entry no session
    /// record matched yet), that entry is its record; otherwise it is a session record until the
    /// file holds it.
    pub fn note_ran(&mut self, command: String) {
        let since = self.rans.saturating_sub(1);
        self.unmatched.retain(|(_, read_at)| *read_at >= since);
        if let Some(i) = self.unmatched.iter().position(|(e, _)| *e == command) {
            self.unmatched.remove(i);
        } else {
            self.record_session(command);
        }
        self.rans += 1;
    }

    /// New entries read from the end of the file; each one removes one equal session record.
    pub fn append_file_entries(&mut self, entries: Vec<String>) {
        for entry in &entries {
            if !self.retire_session_record(entry) {
                if self.unmatched.len() >= MAX_UNMATCHED {
                    self.unmatched.pop_front();
                }
                self.unmatched.push_back((entry.clone(), self.rans));
            }
        }
        for entry in entries {
            if let Some(s) = &self.secret {
                let key = s.key_of(&entry);
                self.file_keys.push(key);
                self.slots.entry(key).or_default().file_newest = Some(self.file.len());
            }
            self.file.push(entry);
        }
    }

    /// A full re-read: replaces the file entries; each new entry removes one equal session record.
    pub fn replace_file_entries(&mut self, entries: Vec<String>) {
        for entry in &entries {
            let _ = self.retire_session_record(entry);
        }
        self.file = entries;
        self.rekey();
    }

    /// The keys of every entry held (empty without a secret).
    pub fn keys(&self) -> impl Iterator<Item = Key> + '_ {
        self.slots.keys().copied()
    }

    /// True when entries carry keys (a secret is set).
    pub fn is_keyed(&self) -> bool {
        self.secret.is_some()
    }

    /// Session records newest first, then file entries newest first.
    pub(crate) fn newest_first(&self) -> impl Iterator<Item = &str> {
        self.session
            .iter()
            .rev()
            .chain(self.file.iter().rev())
            .map(String::as_str)
    }

    /// `newest_first` with each entry's key (`None` without a secret).
    pub(crate) fn newest_first_keyed(&self) -> impl Iterator<Item = (&str, Option<Key>)> {
        let keyed = self.secret.is_some();
        let session = self
            .session
            .iter()
            .enumerate()
            .rev()
            .map(move |(i, e)| (e.as_str(), keyed.then(|| self.session_keys[i])));
        let file = self
            .file
            .iter()
            .enumerate()
            .rev()
            .map(move |(i, e)| (e.as_str(), keyed.then(|| self.file_keys[i])));
        session.chain(file)
    }

    /// The text of `key`, if an entry holds it.
    pub fn text_for(&self, key: Key) -> Option<&str> {
        let slot = self.slots.get(&key)?;
        if let Some(i) = slot.file_newest {
            return Some(&self.file[i]);
        }
        let i = self.session_keys.iter().rposition(|k| *k == key)?;
        Some(&self.session[i])
    }

    /// How new `key`'s newest entry is: larger is newer, and every session record is newer than
    /// every file entry (the recency order of `newest_first`).
    pub(crate) fn recency(&self, key: Key) -> (bool, u64) {
        match self.slots.get(&key) {
            Some(s) if s.session_count > 0 => (true, s.session_seq),
            Some(s) => (false, s.file_newest.map_or(0, |i| i as u64)),
            None => (false, 0),
        }
    }

    /// The entry equal to `text`, if one is held.
    pub fn find(&self, text: &str) -> Option<&str> {
        if let Some(e) = self.session.iter().rev().find(|e| *e == text) {
            return Some(e);
        }
        match &self.secret {
            Some(s) => self.text_for(s.key_of(text)).filter(|e| *e == text),
            None => self
                .file
                .iter()
                .rev()
                .find(|e| *e == text)
                .map(String::as_str),
        }
    }

    /// The key of `text` under the secret, if there is one.
    pub fn key_of(&self, text: &str) -> Option<Key> {
        self.secret.as_ref().map(|s| s.key_of(text))
    }

    /// Recomputes every key and slot from scratch (or clears them without a secret).
    fn rekey(&mut self) {
        self.file_keys.clear();
        self.session_keys.clear();
        self.slots.clear();
        let Some(s) = &self.secret else {
            return;
        };
        for (i, entry) in self.file.iter().enumerate() {
            let key = s.key_of(entry);
            self.file_keys.push(key);
            self.slots.entry(key).or_default().file_newest = Some(i);
        }
        for entry in &self.session {
            let key = s.key_of(entry);
            self.session_keys.push(key);
            self.next_seq += 1;
            let slot = self.slots.entry(key).or_default();
            slot.session_count += 1;
            slot.session_seq = self.next_seq;
        }
    }

    /// The file now holds `entry`, so drop the oldest equal session record, if any (true).
    fn retire_session_record(&mut self, entry: &str) -> bool {
        let Some(i) = self.session.iter().position(|s| s == entry) else {
            return false;
        };
        self.session.remove(i);
        if self.secret.is_some() {
            let key = self.session_keys.remove(i);
            if let Some(slot) = self.slots.get_mut(&key) {
                slot.session_count -= 1;
                if slot.session_count == 0 && slot.file_newest.is_none() {
                    self.slots.remove(&key);
                }
            }
        }
        true
    }

    #[cfg(test)]
    pub(crate) fn file_entries(&self) -> &[String] {
        &self.file
    }

    #[cfg(test)]
    fn session_len(&self) -> usize {
        self.session.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn file(entries: &[&str]) -> History {
        let mut h = History::new();
        h.replace_file_entries(entries.iter().map(|s| s.to_string()).collect());
        h
    }

    #[test]
    fn most_recent_file_match_wins() {
        let h = file(&["git push a", "git pull", "git push b"]);
        assert_eq!(h.latest_with_prefix("git pu"), Some("git push b"));
    }

    #[test]
    fn session_records_rank_above_file_entries() {
        let mut h = file(&["git pull"]);
        h.record_session("git push".into());
        assert_eq!(h.latest_with_prefix("git p"), Some("git push"));
    }

    #[test]
    fn no_match_is_none() {
        assert_eq!(file(&["ls"]).latest_with_prefix("zz"), None);
    }

    #[test]
    fn appended_entry_drops_one_matching_session_record() {
        let mut h = History::new();
        h.record_session("make".into());
        h.append_file_entries(vec!["ls".into(), "make".into()]);
        assert_eq!(h.session_len(), 0);
        assert_eq!(h.latest_with_prefix("ma"), Some("make"));
    }

    #[test]
    fn appended_entry_drops_only_one_of_equal_session_records() {
        let mut h = History::new();
        h.record_session("make".into());
        h.record_session("make".into());
        h.append_file_entries(vec!["make".into()]);
        assert_eq!(h.session_len(), 1);
    }

    #[test]
    fn full_reread_keeps_unwritten_session_records() {
        let mut h = History::new();
        h.record_session("a1".into());
        h.record_session("b1".into());
        h.replace_file_entries(vec!["a1".into()]);
        assert_eq!(h.latest_with_prefix("b"), Some("b1"));
        assert_eq!(h.latest_with_prefix("a"), Some("a1"));
    }

    #[test]
    fn match_is_case_sensitive() {
        assert_eq!(file(&["Git x"]).latest_with_prefix("git"), None);
    }

    #[test]
    fn walk_returns_the_first_candidate_not_skipped() {
        let h = file(&["cd proj", "cd fuxx-cli"]);
        assert_eq!(
            h.latest_with_prefix_except("cd ", |c| c == "cd fuxx-cli"),
            Some("cd proj")
        );
        assert_eq!(
            h.latest_with_prefix_except("cd ", |_| false),
            Some("cd fuxx-cli")
        );
    }

    #[test]
    fn walk_with_every_candidate_skipped_is_none() {
        let h = file(&["cd a", "cd b", "ls"]);
        assert_eq!(h.latest_with_prefix_except("cd", |_| true), None);
    }

    #[test]
    fn walk_keeps_session_before_file_order() {
        let mut h = file(&["cd b"]);
        h.record_session("cd a".into());
        h.record_session("cd x".into());
        assert_eq!(
            h.latest_with_prefix_except("cd", |c| c == "cd x"),
            Some("cd a")
        );
    }

    #[test]
    fn walk_gives_up_after_max_skipped_distinct_candidates() {
        // Newest first: MAX_SKIPPED rejected ones, then a good one.
        let mut entries = vec!["cd good".to_string()];
        entries.extend((0..MAX_SKIPPED).map(|i| format!("cd bad{i}")));
        let h = {
            let mut h = History::new();
            h.replace_file_entries(entries.clone());
            h
        };
        let bad = |c: &str| c.starts_with("cd bad");
        assert_eq!(h.latest_with_prefix_except("cd", bad), None);
        // One fewer rejected candidate: the good one is reached.
        entries.pop();
        let mut h = History::new();
        h.replace_file_entries(entries);
        assert_eq!(h.latest_with_prefix_except("cd", bad), Some("cd good"));
    }

    #[test]
    fn walk_counts_and_checks_a_repeated_rejected_candidate_once() {
        let mut entries = vec!["cd good".to_string()];
        entries.extend((0..MAX_SKIPPED * 3).map(|_| "cd bad".to_string()));
        let mut h = History::new();
        h.replace_file_entries(entries);
        let mut checks = 0;
        let got = h.latest_with_prefix_except("cd", |c| {
            checks += 1;
            c == "cd bad"
        });
        assert_eq!(got, Some("cd good"));
        assert_eq!(checks, 2);
    }

    #[test]
    fn walk_stops_at_the_first_kept_candidate_even_if_exact() {
        // The exact match is the newest kept one; `suggestion()` turns that into no suggestion.
        let h = file(&["cd proj/x", "cd proj"]);
        assert_eq!(
            h.latest_with_prefix_except("cd proj", |_| false),
            Some("cd proj")
        );
    }

    #[test]
    fn walk_keeps_the_next_candidate_unchecked_once_the_time_budget_is_spent() {
        // Newest first: slow0, slow1, slow2, slow3. Each check takes 30 ms and rejects.
        let h = file(&["cd slow3", "cd slow2", "cd slow1", "cd slow0"]);
        let mut checked = Vec::new();
        let got = h.latest_with_prefix_within("cd", Duration::from_millis(50), |c| {
            checked.push(c.to_string());
            std::thread::sleep(Duration::from_millis(30));
            true
        });
        // slow0 at 0 ms, slow1 at ~30 ms; at ~60 ms the budget is spent: slow2 is kept unchecked.
        assert_eq!(got, Some("cd slow2"));
        assert_eq!(checked, ["cd slow0", "cd slow1"]);
    }

    #[test]
    fn walk_within_budget_checks_as_before() {
        let h = file(&["cd good", "cd bad"]);
        assert_eq!(
            h.latest_with_prefix_within("cd", Duration::from_secs(5), |c| c == "cd bad"),
            Some("cd good")
        );
    }

    #[test]
    fn walk_budget_is_50_ms() {
        assert_eq!(CHECK_BUDGET, Duration::from_millis(50));
    }

    /// A judge for walk tests: `cd ..`, `cd ../..` and `cd ~` are held, `cd bad*` skipped,
    /// other `cd`s best, anything else plain.
    fn judge(c: &str) -> Verdict {
        match c {
            "cd .." | "cd ../.." | "cd ~" => Verdict::Hold,
            _ if c.starts_with("cd bad") => Verdict::Skip,
            _ if c.starts_with("cd ") => Verdict::Best,
            _ => Verdict::Plain,
        }
    }

    #[test]
    fn ranked_walk_puts_a_best_candidate_above_a_newer_held_one() {
        let h = file(&["cd proj", "cd ../.."]);
        assert_eq!(h.best_with_prefix("cd ", judge), Some("cd proj"));
    }

    #[test]
    fn ranked_walk_without_a_best_candidate_shows_the_newest_held_one() {
        let h = file(&["cd ~", "cd bad", "cd ..", "cd bad2"]);
        assert_eq!(h.best_with_prefix("cd ", judge), Some("cd .."));
    }

    #[test]
    fn ranked_walk_keeps_session_first_within_a_group() {
        let mut h = file(&["cd proj"]);
        h.record_session("cd src".into());
        h.record_session("cd ..".into());
        assert_eq!(h.best_with_prefix("cd ", judge), Some("cd src"));
    }

    #[test]
    fn ranked_walk_ends_at_a_plain_candidate() {
        // A plain candidate keeps its old place: a newer held one still wins over it, and an
        // older best one is not reached.
        let h = file(&["cd proj", "cdk deploy", "cd .."]);
        assert_eq!(h.best_with_prefix("cd", judge), Some("cd .."));
        let h = file(&["cd proj", "cdk deploy"]);
        assert_eq!(h.best_with_prefix("cd", judge), Some("cdk deploy"));
    }

    #[test]
    fn ranked_walk_returns_the_held_candidate_when_the_bound_is_hit() {
        // Newest first: a held one, MAX_SKIPPED - 1 rejected ones, then a best one.
        let mut entries = vec!["cd good".to_string()];
        entries.extend((0..MAX_SKIPPED - 1).map(|i| format!("cd bad{i}")));
        entries.push("cd ..".to_string());
        let mut h = History::new();
        h.replace_file_entries(entries.clone());
        assert_eq!(h.best_with_prefix("cd", judge), Some("cd .."));
        // One fewer rejected candidate: the best one is reached.
        entries.remove(1);
        let mut h = History::new();
        h.replace_file_entries(entries);
        assert_eq!(h.best_with_prefix("cd", judge), Some("cd good"));
    }

    #[test]
    fn ranked_walk_returns_the_held_candidate_when_the_budget_is_spent() {
        // Newest first: cd .. (held), cd bad (skipped), cd good. Each check takes 30 ms.
        let h = file(&["cd good", "cd bad", "cd .."]);
        let mut checked = Vec::new();
        let got = h.best_with_prefix_within("cd", Duration::from_millis(50), |c| {
            checked.push(c.to_string());
            std::thread::sleep(Duration::from_millis(30));
            judge(c)
        });
        assert_eq!(got, Some("cd .."));
        assert_eq!(checked, ["cd ..", "cd bad"]);
    }

    #[test]
    fn ranked_walk_checks_a_repeated_held_candidate_once() {
        let mut entries = vec!["cd good".to_string()];
        entries.extend((0..MAX_SKIPPED * 3).map(|_| "cd ..".to_string()));
        let mut h = History::new();
        h.replace_file_entries(entries);
        let mut checks = 0;
        let got = h.best_with_prefix("cd", |c| {
            checks += 1;
            judge(c)
        });
        assert_eq!(got, Some("cd good"));
        assert_eq!(checks, 2);
    }

    #[test]
    fn format_parses_zsh_and_bash() {
        assert_eq!("zsh".parse::<HistoryFormat>(), Ok(HistoryFormat::Zsh));
        assert_eq!("bash".parse::<HistoryFormat>(), Ok(HistoryFormat::Bash));
        assert!("fish".parse::<HistoryFormat>().is_err());
    }

    #[test]
    fn ran_already_in_the_file_adds_no_session_record() {
        let mut h = History::new();
        h.append_file_entries(vec!["make".into()]);
        h.note_ran("make".into());
        assert_eq!(h.session_len(), 0);
        // A newer entry another shell wrote wins, as it would in the file alone.
        h.append_file_entries(vec!["make x".into()]);
        assert_eq!(h.latest_with_prefix("make"), Some("make x"));
        // Reports one prompt late: the file gained two commands before either `ran` came.
        h.append_file_entries(vec!["a1".into(), "b1".into()]);
        h.note_ran("a1".into());
        h.note_ran("b1".into());
        assert_eq!(h.session_len(), 0);
    }

    #[test]
    fn ran_not_yet_in_the_file_is_a_session_record() {
        let mut h = History::new();
        h.note_ran("make".into());
        assert_eq!(h.session_len(), 1);
        // Each file entry matches one `ran` only.
        h.append_file_entries(vec!["ls".into()]);
        h.note_ran("ls".into());
        h.note_ran("ls".into());
        assert_eq!(h.session_len(), 2);
    }

    #[test]
    fn an_old_file_entry_never_absorbs_a_ran() {
        // zsh's defaults: growth comes from other shells exiting; their commands must not
        // absorb this shell's report later on (the review's case).
        let mut h = History::new();
        h.append_file_entries(vec!["make test".into(), "make build".into()]);
        h.note_ran("ls".into());
        h.note_ran("pwd".into());
        h.note_ran("make test".into());
        assert_eq!(h.latest_with_prefix("make"), Some("make test"));
    }
}
