//! # fuxx-autosuggestions
//!
//! Grey ghost text that completes the command you are typing from your past commands.
//! See `docs/design.md`.
//!
//! What goes here:
//!
//! - Turning the core's ranked history into **one** suggestion for the current line.
//! - Where a partial accept stops (the next word boundary) and what a full accept inserts.
//!
//! History reading and ranking live in `fuxx-completes-core`. Drawing the ghost text lives in
//! the adapters (`adapters/`) and in fuxx.

use fuxx_completes_core::history::History;

/// The text to show after `buffer`, or None. None for an empty buffer, no match, or an exact match.
pub fn suggestion<'a>(history: &'a History, buffer: &str) -> Option<&'a str> {
    if buffer.is_empty() {
        return None;
    }
    let rest = &history.latest_with_prefix(buffer)?[buffer.len()..];
    (!rest.is_empty()).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hist(entries: &[&str]) -> History {
        let mut h = History::new();
        h.replace_file_entries(entries.iter().map(|s| s.to_string()).collect());
        h
    }

    #[test]
    fn returns_text_after_buffer() {
        let h = hist(&["git push origin main"]);
        assert_eq!(suggestion(&h, "git p"), Some("ush origin main"));
    }

    #[test]
    fn empty_buffer_gives_none() {
        assert_eq!(suggestion(&hist(&["ls"]), ""), None);
    }

    #[test]
    fn exact_match_gives_none() {
        let h = hist(&["git push", "git pull"]);
        assert_eq!(suggestion(&h, "git pull"), None);
        // The most recent match is the exact one; an older longer one is not used.
        let h = hist(&["git pull --rebase", "git pull"]);
        assert_eq!(suggestion(&h, "git pull"), None);
    }

    #[test]
    fn multibyte_prefix_splits_cleanly() {
        assert_eq!(suggestion(&hist(&["echo 日本語"]), "echo 日"), Some("本語"));
    }

    #[test]
    fn multiline_entry_suggests_rest() {
        let h = hist(&["for i\ndo x\ndone"]);
        assert_eq!(suggestion(&h, "for"), Some(" i\ndo x\ndone"));
    }
}
