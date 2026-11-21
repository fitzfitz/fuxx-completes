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
use fuxx_completes_core::history::{History, Verdict, CHECK_BUDGET};
use fuxx_completes_core::rank::walk;
pub use fuxx_completes_core::rank::Ranking;

/// The text to show after `buffer`, or None. None for an empty buffer, no match, or an exact match.
/// A `cd`/`pushd` to a folder that certainly doesn't exist from `place` is passed over for the
/// next match; one down into the current folder ranks above one that works from anywhere, which
/// is shown only when no such match turns up (`docs/design.md` §5.4). Matches are walked in the
/// ranking order of `ranking` (where, how often and how well a command ran, and what followed
/// the previous command); with no context that is newest first.
pub fn suggestion<'a>(
    history: &'a History,
    buffer: &str,
    place: &Place,
    ranking: &Ranking,
) -> Option<&'a str> {
    if buffer.is_empty() {
        return None;
    }
    let candidates = history.ranked_with_prefix(buffer, ranking);
    let entry = walk(candidates, CHECK_BUDGET, |c| match classify(c, place) {
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

    fn none() -> Ranking<'static> {
        Ranking::none(Path::new("/"))
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
        assert_eq!(suggestion(&h, "cd ", &k.place(), &none()), Some("proj"));
    }

    #[test]
    fn typed_text_only_matching_a_cd_up_still_suggests_it() {
        let k = Kerja::new();
        let h = hist(&["cd proj", "cd ../.."]);
        assert_eq!(suggestion(&h, "cd .", &k.place(), &none()), Some("./.."));
    }

    #[test]
    fn without_a_cd_down_here_the_newest_valid_cd_is_suggested() {
        let k = Kerja::new();
        let h = hist(&["cd ~", "cd ..", "cd nope", "cd ../.."]);
        assert_eq!(suggestion(&h, "cd ", &k.place(), &none()), Some("../.."));
        let h = hist(&["cd ../..", "cd ~", "cd nope"]);
        assert_eq!(suggestion(&h, "cd ", &k.place(), &none()), Some("~"));
    }

    #[test]
    fn a_dot_slash_target_counts_as_down_here() {
        let k = Kerja::new();
        let h = hist(&["cd ./proj", "cd ~", "cd .."]);
        assert_eq!(suggestion(&h, "cd ", &k.place(), &none()), Some("./proj"));
    }

    #[test]
    fn a_bare_cd_or_cd_dash_ranks_below_a_cd_down_here() {
        let k = Kerja::new();
        let h = hist(&["cd proj", "cd -", "cd"]);
        assert_eq!(suggestion(&h, "c", &k.place(), &none()), Some("d proj"));
    }

    #[test]
    fn other_commands_keep_the_newest_match() {
        let k = Kerja::new();
        let h = hist(&["git push", "cd proj", "git pull", "cd ../.."]);
        assert_eq!(suggestion(&h, "git p", &k.place(), &none()), Some("ull"));
        let h = hist(&["cd proj", "cat notes"]);
        assert_eq!(suggestion(&h, "c", &k.place(), &none()), Some("at notes"));
    }

    #[test]
    fn cd_to_a_missing_folder_is_passed_over_for_the_next_match() {
        let h = hist(&["cd /usr", "cd /zz-fuxx-surely-missing"]);
        assert_eq!(suggestion(&h, "cd ", &nowhere(), &none()), Some("/usr"));
    }

    #[test]
    fn only_missing_cd_targets_gives_none() {
        let h = hist(&["cd /zz-fuxx-missing-a", "cd /zz-fuxx-missing-b"]);
        assert_eq!(suggestion(&h, "cd ", &nowhere(), &none()), None);
    }

    #[test]
    fn returns_text_after_buffer() {
        let h = hist(&["git push origin main"]);
        assert_eq!(
            suggestion(&h, "git p", &nowhere(), &none()),
            Some("ush origin main")
        );
    }

    #[test]
    fn empty_buffer_gives_none() {
        assert_eq!(suggestion(&hist(&["ls"]), "", &nowhere(), &none()), None);
    }

    #[test]
    fn exact_match_gives_none() {
        let h = hist(&["git push", "git pull"]);
        assert_eq!(suggestion(&h, "git pull", &nowhere(), &none()), None);
        // The most recent match is the exact one; an older longer one is not used.
        let h = hist(&["git pull --rebase", "git pull"]);
        assert_eq!(suggestion(&h, "git pull", &nowhere(), &none()), None);
    }

    #[test]
    fn multibyte_prefix_splits_cleanly() {
        assert_eq!(
            suggestion(&hist(&["echo 日本語"]), "echo 日", &nowhere(), &none()),
            Some("本語")
        );
    }

    #[test]
    fn multiline_entry_suggests_rest() {
        let h = hist(&["for i\ndo x\ndone"]);
        assert_eq!(
            suggestion(&h, "for", &nowhere(), &none()),
            Some(" i\ndo x\ndone")
        );
    }

    /// The ranking order feeds M0's `cd` walk: a `cd ..` that ran here often (tier 1, held)
    /// still gives way to a `cd` down into the current folder, and a tier-1 `cd` to a missing
    /// folder is passed over.
    #[test]
    fn ranked_order_feeds_the_cd_walk() {
        use fuxx_completes_core::context::index::ContextIndex;
        use fuxx_completes_core::context::record::Record;
        use fuxx_completes_core::context::secret::Secret;
        let k = Kerja::new();
        let s = Secret::from_bytes([5; 32]);
        let rec = |c: &str, t: i64| Record {
            key: s.key_of(c),
            folder: k.kerja.clone(),
            status: 0,
            time: t,
        };
        let mut ix = ContextIndex::default();
        for t in 0..5 {
            ix.add(&rec("cd ..", t));
        }
        let mut h = hist(&["cd proj", "cd .."]);
        h.set_secret(Some(&s));
        let r = Ranking {
            index: Some(&ix),
            cwd: &k.kerja,
            previous: None,
            successors: None,
        };
        assert_eq!(suggestion(&h, "cd ", &k.place(), &r), Some("proj"));
        // A tier-1 cd to a folder that isn't there is passed over for the next match.
        let mut ix = ContextIndex::default();
        for t in 0..3 {
            ix.add(&rec("cd nope", t));
        }
        let mut h = hist(&["cd ~", "cd nope"]);
        h.set_secret(Some(&s));
        let r = Ranking {
            index: Some(&ix),
            cwd: &k.kerja,
            previous: None,
            successors: None,
        };
        assert_eq!(suggestion(&h, "cd ", &k.place(), &r), Some("~"));
        // And a plain command ranked first by context is suggested over a newer one.
        let mut ix = ContextIndex::default();
        ix.add(&rec("make test", 1));
        let mut h = hist(&["make test", "make build"]);
        h.set_secret(Some(&s));
        let r = Ranking {
            index: Some(&ix),
            cwd: &k.kerja,
            previous: None,
            successors: None,
        };
        assert_eq!(suggestion(&h, "make ", &k.place(), &r), Some("test"));
    }
}
