//! The in-memory history index: entries read from the shell's history file plus the commands
//! the adapter reported this session.

use std::str::FromStr;

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
    fn session_len(&self) -> usize {
        self.session.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn format_parses_zsh_only() {
        assert_eq!("zsh".parse::<HistoryFormat>(), Ok(HistoryFormat::Zsh));
        assert!("bash".parse::<HistoryFormat>().is_err());
    }
}
