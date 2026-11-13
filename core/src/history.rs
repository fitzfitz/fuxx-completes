//! The in-memory history index: entries read from the shell's history file plus the commands
//! the adapter reported this session.

use std::str::FromStr;
use std::time::{Duration, Instant};

/// How many distinct candidates `latest_with_prefix_except` may reject before it gives up.
pub const MAX_SKIPPED: usize = 50;

/// How long `latest_with_prefix_except` may spend checking candidates.
pub const CHECK_BUDGET: Duration = Duration::from_millis(50);

/// The shell history file formats the core can read. Only zsh in M0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFormat {
    Zsh,
}

impl FromStr for HistoryFormat {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "zsh" => Ok(HistoryFormat::Zsh),
            _ => Err(()),
        }
    }
}

/// Past commands, oldest first. `file` holds what the shell wrote to its history file;
/// `session` holds commands reported by the adapter that the file doesn't hold yet.
#[derive(Debug, Default)]
pub struct History {
    file: Vec<String>,
    session: Vec<String>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Most recent entry starting with `prefix`: session records newest-first, then file
    /// entries newest-first. Plain byte-prefix match, case-sensitive.
    pub fn latest_with_prefix(&self, prefix: &str) -> Option<&str> {
        self.session
            .iter()
            .rev()
            .chain(self.file.iter().rev())
            .map(String::as_str)
            .find(|entry| entry.starts_with(prefix))
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
        let start = Instant::now();
        let mut rejected: Vec<&str> = Vec::new();
        for entry in self
            .session
            .iter()
            .rev()
            .chain(self.file.iter().rev())
            .map(String::as_str)
            .filter(|entry| entry.starts_with(prefix))
        {
            if rejected.contains(&entry) {
                continue;
            }
            if start.elapsed() >= budget || !skip(entry) {
                return Some(entry);
            }
            rejected.push(entry);
            if rejected.len() >= MAX_SKIPPED {
                return None;
            }
        }
        None
    }

    pub fn record_session(&mut self, command: String) {
        self.session.push(command);
    }

    /// New entries read from the end of the file; each one removes one equal session record.
    pub fn append_file_entries(&mut self, entries: Vec<String>) {
        for entry in &entries {
            self.retire_session_record(entry);
        }
        self.file.extend(entries);
    }

    /// A full re-read: replaces the file entries; each new entry removes one equal session record.
    pub fn replace_file_entries(&mut self, entries: Vec<String>) {
        for entry in &entries {
            self.retire_session_record(entry);
        }
        self.file = entries;
    }

    /// The file now holds `entry`, so drop the oldest equal session record, if any.
    fn retire_session_record(&mut self, entry: &str) {
        if let Some(i) = self.session.iter().position(|s| s == entry) {
            self.session.remove(i);
        }
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

    #[test]
    fn format_parses_zsh_only() {
        assert_eq!("zsh".parse::<HistoryFormat>(), Ok(HistoryFormat::Zsh));
        assert!("bash".parse::<HistoryFormat>().is_err());
    }
}
