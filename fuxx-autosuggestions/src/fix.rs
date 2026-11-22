//! A fix after a failed command (M1d §4.4, the user's choice A): grey text on the empty line
//! right after a command failed. Nothing is ever run; the fix is only offered.

use fuxx_completes_core::history::{History, CHECK_BUDGET};
use fuxx_completes_core::rank::{here_order, repo_order, Ranking};
use fuxx_completes_core::safety::{is_dangerous, looks_secret};
use std::time::Instant;

/// The fixed line for `failed`, which just failed in `ranking.cwd`, or `None`. First found wins:
/// the folder's package manager (on any failure), then, only for a line that never succeeded
/// anywhere and isn't a project command, a typo in the command word and the closest successful
/// line here or in this repo (the typo rule only after `status` 127, command not found). Never a dangerous or secret-looking line, never `failed` itself.
pub fn fix_after_failure(
    history: &History,
    failed: &str,
    status: i32,
    ranking: &Ranking,
    projects: &[String],
) -> Option<String> {
    let ok = |c: &String| c != failed && !is_dangerous(c) && !looks_secret(c);
    if let Some(c) = package_manager(failed, projects).filter(ok) {
        return Some(c);
    }
    let index = ranking.index.filter(|_| history.is_keyed())?;
    let succeeded = history
        .key_of(failed)
        .and_then(|k| index.anywhere(k))
        .is_some_and(|t| t.ok > 0);
    if succeeded || projects.iter().any(|p| p == failed) {
        return None;
    }
    // This folder's and this repo's successful lines, in rank order.
    let here_ok: Vec<&str> = here_order(history, "", ranking)
        .into_iter()
        .filter(|t| {
            let tally = history
                .key_of(t)
                .and_then(|k| index.in_folder(ranking.cwd)?.get(&k).copied());
            tally.is_some_and(|t| t.ok > 0)
        })
        .collect();
    let repo_ok = repo_order(history, "", ranking);
    // A typo in the command word means the shell couldn't find it.
    if status == COMMAND_NOT_FOUND {
        if let Some(c) = command_word_typo(history, failed, ranking, projects, &here_ok, &repo_ok) {
            if ok(&c) {
                return Some(c);
            }
        }
    }
    let n = failed.chars().count();
    here_ok
        .iter()
        .chain(&repo_ok)
        .filter(|l| l.chars().count().abs_diff(n) <= 2)
        .map(|l| (edit_distance(l, failed), *l))
        .filter(|(d, _)| (1..=2).contains(d))
        .min_by_key(|(d, _)| *d)
        .map(|(_, l)| l.to_string())
        .filter(ok)
}

/// The exit status a shell gives a command it can't find.
const COMMAND_NOT_FOUND: i32 = 127;

/// The JavaScript package managers and the runner each one uses.
const MANAGERS: [(&str, &str, &str); 4] = [
    ("npm", "npx", "npx"),
    ("pnpm", "pnpx", "pnpm dlx"),
    ("yarn", "", "yarn dlx"),
    ("bun", "bunx", "bunx"),
];

/// Rule 1: `failed` starts with another manager (or its runner) than the folder's, which is the
/// first manager a project command starts with.
fn package_manager(failed: &str, projects: &[String]) -> Option<String> {
    let mut words = failed.split_whitespace();
    let first = words.next()?;
    let (from, runner) = MANAGERS.iter().find_map(|(m, x, _)| {
        if first == *m {
            Some((*m, false))
        } else if !x.is_empty() && first == *x {
            Some((*m, true))
        } else {
            None
        }
    })?;
    let (to, _, to_runner) = projects.iter().find_map(|p| {
        let w = p.split_whitespace().next()?;
        MANAGERS.iter().find(|(m, _, _)| *m == w)
    })?;
    if from == *to {
        return None;
    }
    let args: Vec<&str> = words.collect();
    if runner {
        return Some(
            std::iter::once(*to_runner)
                .chain(args)
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    let mut out: Vec<&str> = vec![to];
    match args.split_first() {
        None => out.push("install"),
        Some((&sub, rest)) => {
            let packages = rest.iter().any(|a| !a.starts_with('-'));
            let sub = match sub {
                "i" | "install" | "add" if packages && *to == "npm" => "install",
                "i" | "install" | "add" if packages => "add",
                "i" | "install" => "install",
                other => other,
            };
            out.push(sub);
            out.extend(rest);
        }
    }
    Some(out.join(" "))
}

/// Rule 2: the failed command word is unknown and within 2 edits (fewer than its length) of a
/// known one: the project's commands', then this folder's, this repo's and every successful
/// line's. The same arguments with that word.
fn command_word_typo(
    history: &History,
    failed: &str,
    ranking: &Ranking,
    projects: &[String],
    here_ok: &[&str],
    repo_ok: &[&str],
) -> Option<String> {
    let first = failed.split_whitespace().next()?;
    let start = failed.find(first)?;
    let rest = &failed[start + first.len()..];
    let mut words: Vec<&str> = Vec::new();
    for l in projects
        .iter()
        .map(String::as_str)
        .chain(here_ok.iter().copied())
    {
        push_first(l, &mut words);
    }
    for l in repo_ok {
        push_first(l, &mut words);
    }
    let mut anywhere: std::collections::HashSet<&str> = std::collections::HashSet::new();
    if let Some(index) = ranking.index {
        let started = Instant::now();
        for key in history.keys() {
            if started.elapsed() >= CHECK_BUDGET {
                break;
            }
            if index.anywhere(key).is_some_and(|t| t.ok > 0) {
                if let Some(w) = history
                    .text_for(key)
                    .and_then(|t| t.split_whitespace().next())
                {
                    anywhere.insert(w);
                }
            }
        }
    }
    let mut anywhere: Vec<&str> = anywhere.into_iter().collect();
    anywhere.sort_unstable();
    words.extend(
        anywhere
            .into_iter()
            .filter(|w| !words.contains(w))
            .collect::<Vec<_>>(),
    );
    if words.contains(&first) {
        return None;
    }
    let n = first.chars().count();
    let best = words
        .iter()
        .map(|w| (edit_distance(first, w), *w))
        .filter(|(d, _)| *d >= 1 && *d <= 2 && *d < n)
        .min_by_key(|(d, _)| *d)?;
    Some(format!("{}{rest}", best.1))
}

/// Adds `line`'s first word to `words` once.
fn push_first<'a>(line: &'a str, words: &mut Vec<&'a str>) {
    if let Some(w) = line.split_whitespace().next() {
        if !words.contains(&w) {
            words.push(w);
        }
    }
}

/// Optimal string alignment distance over characters: insertions, deletions, substitutions and
/// swaps of two neighbours each cost 1.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let w = b.len() + 1;
    let mut d = vec![0usize; (a.len() + 1) * w];
    for i in 0..=a.len() {
        d[i * w] = i;
    }
    for (j, cell) in d.iter_mut().enumerate().take(w) {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (d[(i - 1) * w + j] + 1)
                .min(d[i * w + j - 1] + 1)
                .min(d[(i - 1) * w + j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(d[(i - 2) * w + j - 2] + 1);
            }
            d[i * w + j] = v;
        }
    }
    d[a.len() * w + b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuxx_completes_core::context::index::ContextIndex;
    use fuxx_completes_core::context::record::Record;
    use fuxx_completes_core::context::secret::Secret;
    use std::path::{Path, PathBuf};

    /// History `file` (oldest first), keyed, and records `(command, folder, status)`.
    fn ctx(file: &[&str], recs: &[(&str, &str, i32)]) -> (History, ContextIndex) {
        let s = Secret::from_bytes([5; 32]);
        let mut h = History::new();
        h.replace_file_entries(file.iter().map(|e| e.to_string()).collect());
        h.set_secret(Some(&s));
        let mut ix = ContextIndex::default();
        for (t, &(c, f, status)) in recs.iter().enumerate() {
            ix.add(&Record {
                key: s.key_of(c),
                folder: PathBuf::from(f),
                status,
                time: t as i64,
            });
        }
        (h, ix)
    }

    fn here(ix: &ContextIndex) -> Ranking<'_> {
        Ranking {
            index: Some(ix),
            cwd: Path::new("/p"),
            previous: None,
            successors: None,
            repo: None,
        }
    }

    /// The fix after `failed` ended with 127 (command not found).
    fn fix(h: &History, failed: &str, r: &Ranking, projects: &[&str]) -> Option<String> {
        fix_with(h, failed, 127, r, projects)
    }

    fn fix_with(
        h: &History,
        failed: &str,
        status: i32,
        r: &Ranking,
        projects: &[&str],
    ) -> Option<String> {
        let p: Vec<String> = projects.iter().map(|s| s.to_string()).collect();
        fix_after_failure(h, failed, status, r, &p)
    }

    /// A typo in the command word means the shell couldn't find it (127): `sh -c 'exit 1'` is a
    /// real command that failed, not a typo of `ls`.
    #[test]
    fn typo_rule_needs_command_not_found() {
        let (h, ix) = ctx(
            &["ls", "cargo test"],
            &[("ls", "/p", 0), ("cargo test", "/q", 0)],
        );
        let r = here(&ix);
        assert_eq!(fix_with(&h, "sh -c 'exit 1'", 1, &r, &[]), None);
        assert_eq!(fix_with(&h, "cagro test", 1, &r, &[]), None);
        assert_eq!(
            fix_with(&h, "cagro test", 127, &r, &[]).as_deref(),
            Some("cargo test")
        );
        // Rules 1 and 3 fire on any failure.
        let (h, ix) = ctx(&["git push"], &[("git push", "/p", 0)]);
        assert_eq!(
            fix_with(&h, "git psuh", 1, &here(&ix), &[]).as_deref(),
            Some("git push")
        );
        assert_eq!(
            fix_with(&h, "npm i", 1, &here(&ix), &["pnpm install"]).as_deref(),
            Some("pnpm install")
        );
    }

    #[test]
    fn package_manager_swap() {
        let (h, ix) = ctx(&[], &[]);
        let r = here(&ix);
        let pnpm = ["pnpm install", "pnpm dev", "pnpm run dev"];
        assert_eq!(
            fix(&h, "npm run dev", &r, &pnpm).as_deref(),
            Some("pnpm run dev")
        );
        assert_eq!(fix(&h, "npm i", &r, &pnpm).as_deref(), Some("pnpm install"));
        assert_eq!(
            fix(&h, "npm install react", &r, &pnpm).as_deref(),
            Some("pnpm add react")
        );
        assert_eq!(
            fix(&h, "npx vite", &r, &pnpm).as_deref(),
            Some("pnpm dlx vite")
        );
        let npm = ["npm install", "npm test", "npm run dev"];
        assert_eq!(fix(&h, "npm run dev", &r, &npm), None);
        assert_eq!(
            fix(&h, "yarn add react", &r, &npm).as_deref(),
            Some("npm install react")
        );
        assert_eq!(
            fix(&h, "pnpx vite", &r, &["bun install"]).as_deref(),
            Some("bunx vite")
        );
        // No JavaScript project here: nothing.
        assert_eq!(fix(&h, "npm run dev", &r, &["cargo test"]), None);
    }

    #[test]
    fn typo_in_the_command_word() {
        let (h, ix) = ctx(
            &["cargo test", "git status"],
            &[("cargo test", "/q", 0), ("git status", "/q", 0)],
        );
        let r = here(&ix);
        assert_eq!(
            fix(&h, "cagro test", &r, &[]).as_deref(),
            Some("cargo test")
        );
        assert_eq!(
            fix(&h, "gti log -1", &r, &[]).as_deref(),
            Some("git log -1")
        );
        // A project command's word counts too.
        assert_eq!(
            fix(&h, "mkae build", &r, &["make build"]).as_deref(),
            Some("make build")
        );
    }

    #[test]
    fn closest_successful_line() {
        let (h, ix) = ctx(&["git push", "git pull"], &[("git push", "/p", 0)]);
        let r = here(&ix);
        assert_eq!(fix(&h, "git psuh", &r, &[]).as_deref(), Some("git push"));
        // Only lines that succeeded here or in the repo.
        let (h, ix) = ctx(&["git push"], &[("git push", "/q", 0)]);
        assert_eq!(fix(&h, "git psuh", &here(&ix), &[]), None);
    }

    /// Rules 2 and 3 only fire for a line that never succeeded anywhere: a real failure gets no
    /// "fix".
    #[test]
    fn never_for_a_real_failure() {
        let (h, ix) = ctx(
            &["make test", "make tests", "ls"],
            &[
                ("make test", "/p", 0),
                ("make tests", "/p", 0),
                ("ls", "/p", 0),
            ],
        );
        let r = here(&ix);
        assert_eq!(fix(&h, "make test", &r, &[]), None);
        // `cd` is a known word, so `cd nope` is no typo of `ls nope`.
        let (h, ix) = ctx(&["ls", "cd src"], &[("ls", "/p", 0), ("cd src", "/p", 0)]);
        assert_eq!(fix(&h, "cd nope", &here(&ix), &[]), None);
        // Without a context index, only rule 1 can fire.
        let h = History::new();
        let r = Ranking::none(Path::new("/p"));
        assert_eq!(fix(&h, "cagro test", &r, &["cargo test"]), None);
    }

    #[test]
    fn never_dangerous_secret_or_itself() {
        let (h, ix) = ctx(
            &["rm -rf target", "export TOKEN=abcdef"],
            &[("rm -rf target", "/p", 0), ("export TOKEN=abcdef", "/p", 0)],
        );
        let r = here(&ix);
        assert_eq!(fix(&h, "rm -rf targte", &r, &[]), None);
        assert_eq!(fix(&h, "export TOKEN=abcdeg", &r, &[]), None);
        assert_eq!(fix(&h, "rn -rf target", &r, &[]), None);
    }

    #[test]
    fn edit_distance_counts_a_swap_as_one() {
        assert_eq!(edit_distance("gti", "git"), 1);
        assert_eq!(edit_distance("cagro", "cargo"), 1);
        assert_eq!(edit_distance("sl", "ls"), 1);
        assert_eq!(edit_distance("cd", "ls"), 2);
        assert_eq!(edit_distance("", "abc"), 3);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("é", "e"), 1);
    }
}
