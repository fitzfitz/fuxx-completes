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

mod fix;
pub use fix::{edit_distance, fix_after_failure};

use fuxx_completes_core::cd_target::{classify, CdKind, Place};
use fuxx_completes_core::history::{History, Verdict, CHECK_BUDGET};
pub use fuxx_completes_core::rank::Ranking;
use fuxx_completes_core::rank::{follows_of, walk};
use fuxx_completes_core::safety::hidden_for;
use std::time::Instant;

/// The text to show after `buffer`, or None. None for an empty buffer, no match, or an exact match.
/// A `cd`/`pushd` to a folder that certainly doesn't exist from `place` is passed over for the
/// next match; one down into the current folder ranks above one that works from anywhere, which
/// is shown only when no such match turns up (`docs/design.md` §5.4). Matches are walked in the
/// ranking order of `ranking` (where, how often and how well a command ran, and what followed
/// the previous command); with no context that is newest first. In three steps (M1b): this
/// folder's history, then the first of `projects` (the project's commands) starting with the
/// typed text and longer than it, then the rest of history, walked as one sequence.
pub fn suggestion<'a>(
    history: &'a History,
    buffer: &str,
    place: &Place,
    ranking: &Ranking,
    projects: &'a [String],
) -> Option<&'a str> {
    let entry = best_entry(history, buffer, place, ranking, projects)?;
    let rest = &entry[buffer.len()..];
    (!rest.is_empty()).then_some(rest)
}

/// The whole line the ghost text completes to (see `suggestion`).
fn best_entry<'a>(
    history: &'a History,
    buffer: &str,
    place: &Place,
    ranking: &Ranking,
    projects: &'a [String],
) -> Option<&'a str> {
    if buffer.is_empty() {
        return None;
    }
    let judge = |c: &str| match classify(c, place) {
        CdKind::NotCd => Verdict::Plain,
        CdKind::Missing => Verdict::Skip,
        CdKind::DownHere => Verdict::Best,
        CdKind::Anywhere => Verdict::Hold,
    };
    // One walk, three steps (M1b, design §5.4), so M0's `cd` rules and its one budget hold across
    // them: this folder's history; the project's commands longer than the typed text; the rest.
    let here = history.ranked_here(buffer, ranking);
    let repo = history.ranked_repo(buffer, ranking);
    let project = projects
        .iter()
        .filter(|p| p.len() > buffer.len() && p.starts_with(buffer))
        .map(String::as_str);
    let others = history
        .ranked_with_prefix(buffer, ranking)
        .filter(|c| !here.contains(c) && !repo.contains(c));
    // Hidden lines (M1d) leave before the walk: they never count toward its bounds and are never
    // returned unchecked by its budget.
    let candidates = here
        .iter()
        .copied()
        .chain(repo.iter().copied())
        .chain(project)
        .chain(others)
        .filter(|c| !hidden_for(c, buffer));
    walk(candidates, CHECK_BUDGET, judge)
}

/// The suggestion list (M1c, `docs/superpowers/specs/2026-11-22-suggestion-list-design.md`): up to
/// `max` whole command lines. For a typed buffer, item 1 is the line the ghost text completes to,
/// then the same sequence (this folder's history, the project's commands, the rest) without `cd`s
/// to missing folders or the buffer itself; on an empty line, what usually follows the previous
/// command, then this folder's history, the project's commands and the newest history. Distinct,
/// within the walk's time budget.
pub fn suggestion_list<'a>(
    history: &'a History,
    buffer: &str,
    place: &Place,
    ranking: &Ranking,
    projects: &'a [String],
    max: usize,
) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = Vec::new();
    if max == 0 {
        return out;
    }
    let start = Instant::now();
    if let Some(first) = best_entry(history, buffer, place, ranking, projects) {
        if first != buffer {
            out.push(first);
        }
    }
    let follows = if buffer.is_empty() {
        follows_of(history, "", ranking)
    } else {
        Vec::new()
    };
    let here = history.ranked_here(buffer, ranking);
    let repo = history.ranked_repo(buffer, ranking);
    let project = projects
        .iter()
        .filter(|p| p.len() > buffer.len() && p.starts_with(buffer))
        .map(String::as_str);
    let others = history
        .ranked_with_prefix(buffer, ranking)
        .filter(|c| !here.contains(c) && !repo.contains(c));
    for c in follows
        .iter()
        .copied()
        .chain(here.iter().copied())
        .chain(repo.iter().copied())
        .chain(project)
        .chain(others)
    {
        if out.len() >= max || start.elapsed() >= CHECK_BUDGET {
            break;
        }
        if c.is_empty() || c == buffer || out.contains(&c) || hidden_for(c, buffer) {
            continue;
        }
        if classify(c, place) == CdKind::Missing {
            continue;
        }
        out.push(c);
    }
    out
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
        assert_eq!(
            suggestion(&h, "cd ", &k.place(), &none(), &[]),
            Some("proj")
        );
    }

    #[test]
    fn typed_text_only_matching_a_cd_up_still_suggests_it() {
        let k = Kerja::new();
        let h = hist(&["cd proj", "cd ../.."]);
        assert_eq!(
            suggestion(&h, "cd .", &k.place(), &none(), &[]),
            Some("./..")
        );
    }

    #[test]
    fn without_a_cd_down_here_the_newest_valid_cd_is_suggested() {
        let k = Kerja::new();
        let h = hist(&["cd ~", "cd ..", "cd nope", "cd ../.."]);
        assert_eq!(
            suggestion(&h, "cd ", &k.place(), &none(), &[]),
            Some("../..")
        );
        let h = hist(&["cd ../..", "cd ~", "cd nope"]);
        assert_eq!(suggestion(&h, "cd ", &k.place(), &none(), &[]), Some("~"));
    }

    #[test]
    fn a_dot_slash_target_counts_as_down_here() {
        let k = Kerja::new();
        let h = hist(&["cd ./proj", "cd ~", "cd .."]);
        assert_eq!(
            suggestion(&h, "cd ", &k.place(), &none(), &[]),
            Some("./proj")
        );
    }

    #[test]
    fn a_bare_cd_or_cd_dash_ranks_below_a_cd_down_here() {
        let k = Kerja::new();
        let h = hist(&["cd proj", "cd -", "cd"]);
        assert_eq!(
            suggestion(&h, "c", &k.place(), &none(), &[]),
            Some("d proj")
        );
    }

    #[test]
    fn other_commands_keep_the_newest_match() {
        let k = Kerja::new();
        let h = hist(&["git push", "cd proj", "git pull", "cd ../.."]);
        assert_eq!(
            suggestion(&h, "git p", &k.place(), &none(), &[]),
            Some("ull")
        );
        let h = hist(&["cd proj", "cat notes"]);
        assert_eq!(
            suggestion(&h, "c", &k.place(), &none(), &[]),
            Some("at notes")
        );
    }

    #[test]
    fn cd_to_a_missing_folder_is_passed_over_for_the_next_match() {
        let h = hist(&["cd /usr", "cd /zz-fuxx-surely-missing"]);
        assert_eq!(
            suggestion(&h, "cd ", &nowhere(), &none(), &[]),
            Some("/usr")
        );
    }

    #[test]
    fn only_missing_cd_targets_gives_none() {
        let h = hist(&["cd /zz-fuxx-missing-a", "cd /zz-fuxx-missing-b"]);
        assert_eq!(suggestion(&h, "cd ", &nowhere(), &none(), &[]), None);
    }

    #[test]
    fn returns_text_after_buffer() {
        let h = hist(&["git push origin main"]);
        assert_eq!(
            suggestion(&h, "git p", &nowhere(), &none(), &[]),
            Some("ush origin main")
        );
    }

    #[test]
    fn empty_buffer_gives_none() {
        assert_eq!(
            suggestion(&hist(&["ls"]), "", &nowhere(), &none(), &[]),
            None
        );
    }

    #[test]
    fn exact_match_gives_none() {
        let h = hist(&["git push", "git pull"]);
        assert_eq!(suggestion(&h, "git pull", &nowhere(), &none(), &[]), None);
        // The most recent match is the exact one; an older longer one is not used.
        let h = hist(&["git pull --rebase", "git pull"]);
        assert_eq!(suggestion(&h, "git pull", &nowhere(), &none(), &[]), None);
    }

    #[test]
    fn multibyte_prefix_splits_cleanly() {
        assert_eq!(
            suggestion(&hist(&["echo 日本語"]), "echo 日", &nowhere(), &none(), &[]),
            Some("本語")
        );
    }

    #[test]
    fn multiline_entry_suggests_rest() {
        let h = hist(&["for i\ndo x\ndone"]);
        assert_eq!(
            suggestion(&h, "for", &nowhere(), &none(), &[]),
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
            repo: None,
        };
        assert_eq!(suggestion(&h, "cd ", &k.place(), &r, &[]), Some("proj"));
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
            repo: None,
        };
        assert_eq!(suggestion(&h, "cd ", &k.place(), &r, &[]), Some("~"));
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
            repo: None,
        };
        assert_eq!(suggestion(&h, "make ", &k.place(), &r, &[]), Some("test"));
    }

    // ---- M1b: this folder's history, then project commands, then the rest ----

    use fuxx_completes_core::context::index::ContextIndex as Ix;
    use fuxx_completes_core::context::record::Record as Rec;
    use fuxx_completes_core::context::secret::Secret as Sec;

    /// History `file` (oldest first) keyed with a test secret, and records `(command, folder,
    /// status)`.
    fn ctx(file: &[&str], recs: &[(&str, &str, i32)]) -> (History, Ix) {
        let s = Sec::from_bytes([3; 32]);
        let mut h = hist(file);
        h.set_secret(Some(&s));
        let mut ix = Ix::default();
        for (t, &(c, f, status)) in recs.iter().enumerate() {
            ix.add(&Rec {
                key: s.key_of(c),
                folder: std::path::PathBuf::from(f),
                status,
                time: t as i64,
            });
        }
        (h, ix)
    }

    fn here(ix: &Ix) -> Ranking<'_> {
        Ranking {
            index: Some(ix),
            cwd: Path::new("/p"),
            previous: None,
            successors: None,
            repo: None,
        }
    }

    fn cargo() -> Vec<String> {
        ["cargo build", "cargo test", "cargo run"]
            .map(String::from)
            .to_vec()
    }

    #[test]
    fn this_folders_history_beats_project_commands() {
        let (h, ix) = ctx(
            &["cargo test --release"],
            &[("cargo test --release", "/p", 0)],
        );
        let p = cargo();
        assert_eq!(
            suggestion(&h, "cargo t", &nowhere(), &here(&ix), &p),
            Some("est --release")
        );
    }

    #[test]
    fn project_command_beats_history_from_elsewhere() {
        let (h, ix) = ctx(&["cargo tree"], &[("cargo tree", "/other", 0)]);
        let p = cargo();
        assert_eq!(
            suggestion(&h, "cargo t", &nowhere(), &here(&ix), &p),
            Some("est")
        );
    }

    #[test]
    fn history_from_elsewhere_still_shows_without_a_project_match() {
        let (h, ix) = ctx(&["git status"], &[]);
        let p = cargo();
        assert_eq!(
            suggestion(&h, "git st", &nowhere(), &here(&ix), &p),
            Some("atus")
        );
    }

    #[test]
    fn failed_only_here_does_not_hide_project_commands() {
        let (h, ix) = ctx(&["cargo txx"], &[("cargo txx", "/p", 101)]);
        let p = cargo();
        assert_eq!(
            suggestion(&h, "cargo t", &nowhere(), &here(&ix), &p),
            Some("est")
        );
    }

    #[test]
    fn neutral_here_counts_as_this_folders_history() {
        let (h, ix) = ctx(&["cargo tarpaulin"], &[("cargo tarpaulin", "/p", 130)]);
        let p = cargo();
        assert_eq!(
            suggestion(&h, "cargo t", &nowhere(), &here(&ix), &p),
            Some("arpaulin")
        );
    }

    #[test]
    fn exact_project_command_is_skipped_for_a_longer_one() {
        let (h, ix) = ctx(&[], &[]);
        let p: Vec<String> = ["npm run dev", "npm run dev:host"]
            .map(String::from)
            .to_vec();
        assert_eq!(
            suggestion(&h, "npm run dev", &nowhere(), &here(&ix), &p),
            Some(":host")
        );
        assert_eq!(suggestion(&h, "", &nowhere(), &here(&ix), &p), None);
    }

    #[test]
    fn without_a_secret_project_commands_come_before_other_history() {
        let h = hist(&["cargo tree"]);
        let p = cargo();
        let r = Ranking::none(Path::new("/p"));
        assert_eq!(suggestion(&h, "cargo t", &nowhere(), &r, &p), Some("est"));
    }

    // ---- M1c: the suggestion list ----

    #[test]
    fn list_starts_with_the_ghost_text() {
        let h = hist(&["git status", "git stash", "git push"]);
        let r = Ranking::none(Path::new("/p"));
        assert_eq!(
            suggestion_list(&h, "git s", &nowhere(), &r, &[], 5),
            ["git stash", "git status"]
        );
    }

    #[test]
    fn list_is_capped_and_distinct() {
        let entries: Vec<String> = (0..8).map(|i| format!("echo {i}")).collect();
        let mut all: Vec<&str> = entries.iter().map(String::as_str).collect();
        all.extend(["echo 7", "echo 6"]);
        let h = hist(&all);
        let r = Ranking::none(Path::new("/p"));
        let l = suggestion_list(&h, "echo", &nowhere(), &r, &[], 5);
        assert_eq!(l, ["echo 6", "echo 7", "echo 5", "echo 4", "echo 3"]);
        assert!(suggestion_list(&h, "echo", &nowhere(), &r, &[], 0).is_empty());
    }

    #[test]
    fn list_skips_missing_cd_targets_and_the_buffer_itself() {
        let h = hist(&["cd /usr", "cd /zz-fuxx-surely-missing", "make", "make test"]);
        let r = Ranking::none(Path::new("/p"));
        assert_eq!(
            suggestion_list(&h, "cd ", &nowhere(), &r, &[], 5),
            ["cd /usr"]
        );
        assert_eq!(
            suggestion_list(&h, "make", &nowhere(), &r, &[], 5),
            ["make test"]
        );
    }

    #[test]
    fn list_follows_the_three_steps() {
        let (h, ix) = ctx(
            &["cargo test --release", "cargo tree"],
            &[("cargo test --release", "/p", 0)],
        );
        let p = vec!["cargo build".to_string()];
        let l = suggestion_list(&h, "cargo ", &nowhere(), &here(&ix), &p, 5);
        assert_eq!(l, ["cargo test --release", "cargo build", "cargo tree"]);
    }

    #[test]
    fn empty_line_list_order() {
        let (h, ix) = ctx(
            &["ls -la", "git commit", "make here"],
            &[("make here", "/p", 0)],
        );
        let mut succ = fuxx_completes_core::rank::Successors::new();
        succ.record("git add .", "git commit");
        let r = Ranking {
            index: Some(&ix),
            cwd: Path::new("/p"),
            previous: Some("git add ."),
            successors: Some(&succ),
            repo: None,
        };
        let p = vec!["cargo build".to_string()];
        let l = suggestion_list(&h, "", &nowhere(), &r, &p, 5);
        assert_eq!(l, ["git commit", "make here", "cargo build", "ls -la"]);
    }

    #[test]
    fn empty_line_without_anything_is_empty() {
        let h = hist(&[]);
        let r = Ranking::none(Path::new("/p"));
        assert!(suggestion_list(&h, "", &nowhere(), &r, &[], 5).is_empty());
    }

    /// M1d §4.1: a secret-looking line is never ghost text or a list item.
    #[test]
    fn secret_lines_are_never_suggested_or_listed() {
        let h = hist(&["export PATH=x", "export TOKEN=abc123"]);
        assert_eq!(
            suggestion(&h, "export ", &nowhere(), &none(), &[]),
            Some("PATH=x")
        );
        let list = suggestion_list(&h, "export ", &nowhere(), &none(), &[], 5);
        assert_eq!(list, ["export PATH=x"]);
        let list = suggestion_list(&h, "", &nowhere(), &none(), &[], 5);
        assert_eq!(list, ["export PATH=x"]);
    }

    /// Hidden lines never count toward the walk's 50-candidate bound.
    #[test]
    fn fifty_secret_lines_do_not_use_up_the_walk() {
        let mut entries = vec!["export PATH=x".to_string()];
        entries.extend((0..51).map(|i| format!("export TOKEN=a{i}")));
        let mut h = History::new();
        h.replace_file_entries(entries);
        assert_eq!(
            suggestion(&h, "export ", &nowhere(), &none(), &[]),
            Some("PATH=x")
        );
    }

    /// M1d §4.2: typing `g` never puts `git push --force` one key away.
    #[test]
    fn dangerous_needs_its_dangerous_part_typed() {
        let h = hist(&["git pull", "git push --force"]);
        let at = |b: &str| suggestion(&h, b, &nowhere(), &none(), &[]);
        assert_eq!(at("g"), Some("it pull"));
        assert_eq!(at("git p"), Some("ull"));
        assert_eq!(at("git push -"), Some("-force"));
        assert_eq!(at("git push --force"), None);
        let list = suggestion_list(&h, "git p", &nowhere(), &none(), &[], 5);
        assert_eq!(list, ["git pull"]);
    }

    #[test]
    fn no_dangerous_line_on_the_empty_line_list() {
        let h = hist(&["git add .", "git push -f", "git status"]);
        let mut succ = fuxx_completes_core::rank::Successors::new();
        succ.record("git add .", "git push -f");
        let r = Ranking {
            index: None,
            cwd: Path::new("/p"),
            previous: Some("git add ."),
            successors: Some(&succ),
            repo: None,
        };
        let list = suggestion_list(&h, "", &nowhere(), &r, &[], 5);
        assert_eq!(list, ["git status", "git add ."]);
    }

    /// M1d §4.3: the repo's history comes before the project's commands.
    #[test]
    fn repo_history_beats_project_commands() {
        let (h, ix) = ctx(&["cargo tree"], &[]);
        let s = Sec::from_bytes([3; 32]);
        let repo: std::collections::HashMap<_, _> = [(
            s.key_of("cargo tree"),
            fuxx_completes_core::context::index::Tally {
                ok: 1,
                failed: 0,
                last_time: 1,
            },
        )]
        .into_iter()
        .collect();
        let mut r = here(&ix);
        assert_eq!(
            suggestion(&h, "cargo t", &nowhere(), &r, &cargo()),
            Some("est")
        );
        r.repo = Some(&repo);
        assert_eq!(
            suggestion(&h, "cargo t", &nowhere(), &r, &cargo()),
            Some("ree")
        );
        let projects = cargo();
        let list = suggestion_list(&h, "cargo ", &nowhere(), &r, &projects, 5);
        assert_eq!(list[0], "cargo tree");
        let list = suggestion_list(&h, "", &nowhere(), &r, &projects, 5);
        assert_eq!(list[0], "cargo tree");
    }
}
