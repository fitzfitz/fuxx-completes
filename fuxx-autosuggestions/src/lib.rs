//! # fuxx-autosuggestions
//!
//! Grey ghost text that completes the command you are typing from your past commands.
//! See `docs/design.md`.
//!
//! Implemented so far (M0): `suggestion()`, which picks the text to show after the typed buffer
//! from the core's history index.
//!
//! What goes here:
//!
//! - Turning the core's ranked history into **one** suggestion for the current line.
//! - Where a partial accept stops (the next word boundary) and what a full accept inserts.
//!
//! History reading and ranking live in `fuxx-completes-core`. Drawing the ghost text lives in
//! the adapters (`adapters/`) and in fuxx.

use fuxx_completes_core::cd_target::{certainly_missing, Place};
use fuxx_completes_core::history::History;

/// The text to show after `buffer`, or None. None for an empty buffer, no match, or an exact match.
/// A `cd`/`pushd` to a folder that certainly doesn't exist from `place` is passed over for the
/// next match (`docs/design.md` §5.4).
pub fn suggestion<'a>(history: &'a History, buffer: &str, place: &Place) -> Option<&'a str> {
    if buffer.is_empty() {
        return None;
    }
    let entry = history.latest_with_prefix_except(buffer, |c| certainly_missing(c, place))?;
    let rest = &entry[buffer.len()..];
    (!rest.is_empty()).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn hist(entries: &[&str]) -> History {
        let mut h = History::new();
        h.replace_file_entries(entries.iter().map(|s| s.to_string()).collect());
        h
    }

    /// A place where only absolute targets can be checked (no cwd, no home, no cdpath).
    fn nowhere() -> Place<'static> {
        Place {
            cwd: Path::new(""),
            home: None,
            cdpath: None,
        }
    }

    #[test]
    fn cd_to_a_missing_folder_is_passed_over_for_the_next_match() {
        let h = hist(&["cd /usr", "cd /zz-fuxx-surely-missing"]);
        assert_eq!(suggestion(&h, "cd ", &nowhere()), Some("/usr"));
    }

    #[test]
    fn only_missing_cd_targets_gives_none() {
        let h = hist(&["cd /zz-fuxx-missing-a", "cd /zz-fuxx-missing-b"]);
        assert_eq!(suggestion(&h, "cd ", &nowhere()), None);
    }

    #[test]
    fn returns_text_after_buffer() {
        let h = hist(&["git push origin main"]);
        assert_eq!(suggestion(&h, "git p", &nowhere()), Some("ush origin main"));
    }

    #[test]
    fn empty_buffer_gives_none() {
        assert_eq!(suggestion(&hist(&["ls"]), "", &nowhere()), None);
    }

    #[test]
    fn exact_match_gives_none() {
        let h = hist(&["git push", "git pull"]);
        assert_eq!(suggestion(&h, "git pull", &nowhere()), None);
        // The most recent match is the exact one; an older longer one is not used.
        let h = hist(&["git pull --rebase", "git pull"]);
        assert_eq!(suggestion(&h, "git pull", &nowhere()), None);
    }

    #[test]
    fn multibyte_prefix_splits_cleanly() {
        assert_eq!(
            suggestion(&hist(&["echo 日本語"]), "echo 日", &nowhere()),
            Some("本語")
        );
    }

    #[test]
    fn multiline_entry_suggests_rest() {
        let h = hist(&["for i\ndo x\ndone"]);
        assert_eq!(suggestion(&h, "for", &nowhere()), Some(" i\ndo x\ndone"));
    }
}
