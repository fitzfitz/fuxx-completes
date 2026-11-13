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

use fuxx_completes_core::cd_target::{classify, CdKind, Place};
use fuxx_completes_core::history::{History, Verdict};

/// The text to show after `buffer`, or None. None for an empty buffer, no match, or an exact match.
/// A `cd`/`pushd` to a folder that certainly doesn't exist from `place` is passed over for the
/// next match; one down into the current folder ranks above one that works from anywhere, which
/// is shown only when no such match turns up (`docs/design.md` §5.4).
pub fn suggestion<'a>(history: &'a History, buffer: &str, place: &Place) -> Option<&'a str> {
    if buffer.is_empty() {
        return None;
    }
    let entry = history.best_with_prefix(buffer, |c| match classify(c, place) {
        CdKind::NotCd => Verdict::Plain,
        CdKind::Missing => Verdict::Skip,
        CdKind::DownHere => Verdict::Best,
        CdKind::Anywhere => Verdict::Hold,
    })?;
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

    /// `<tmp>/kerja/proj` and `<tmp>/home`; the place is `kerja` with that home and no cdpath.
    struct Kerja {
        _dir: tempfile::TempDir,
        kerja: std::path::PathBuf,
        home: std::path::PathBuf,
    }

    impl Kerja {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = std::fs::canonicalize(dir.path()).unwrap();
            let kerja = root.join("kerja");
            std::fs::create_dir_all(kerja.join("proj")).unwrap();
            let home = root.join("home");
            std::fs::create_dir_all(&home).unwrap();
            Kerja {
                _dir: dir,
                kerja,
                home,
            }
        }

        fn place(&self) -> Place<'_> {
            Place {
                cwd: &self.kerja,
                home: Some(&self.home),
                cdpath: Some(&[]),
            }
        }
    }

    /// The user's report after v0.0.4: in `Kerja`, `cd ` suggested `../..` (the newest valid
    /// `cd`); a `cd` down into the current folder now ranks above one that works from anywhere.
    #[test]
    fn cd_down_into_the_current_folder_ranks_above_a_newer_cd_up() {
        let k = Kerja::new();
        let h = hist(&["cd proj", "cd ../.."]);
        assert_eq!(suggestion(&h, "cd ", &k.place()), Some("proj"));
    }

    #[test]
    fn typed_text_only_matching_a_cd_up_still_suggests_it() {
        let k = Kerja::new();
        let h = hist(&["cd proj", "cd ../.."]);
        assert_eq!(suggestion(&h, "cd .", &k.place()), Some("./.."));
    }

    #[test]
    fn without_a_cd_down_here_the_newest_valid_cd_is_suggested() {
        let k = Kerja::new();
        let h = hist(&["cd ~", "cd ..", "cd nope", "cd ../.."]);
        assert_eq!(suggestion(&h, "cd ", &k.place()), Some("../.."));
        let h = hist(&["cd ../..", "cd ~", "cd nope"]);
        assert_eq!(suggestion(&h, "cd ", &k.place()), Some("~"));
    }

    #[test]
    fn a_dot_slash_target_counts_as_down_here() {
        let k = Kerja::new();
        let h = hist(&["cd ./proj", "cd ~", "cd .."]);
        assert_eq!(suggestion(&h, "cd ", &k.place()), Some("./proj"));
    }

    #[test]
    fn a_bare_cd_or_cd_dash_ranks_below_a_cd_down_here() {
        let k = Kerja::new();
        let h = hist(&["cd proj", "cd -", "cd"]);
        assert_eq!(suggestion(&h, "c", &k.place()), Some("d proj"));
    }

    #[test]
    fn other_commands_keep_the_newest_match() {
        let k = Kerja::new();
        let h = hist(&["git push", "cd proj", "git pull", "cd ../.."]);
        assert_eq!(suggestion(&h, "git p", &k.place()), Some("ull"));
        let h = hist(&["cd proj", "cat notes"]);
        assert_eq!(suggestion(&h, "c", &k.place()), Some("at notes"));
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
