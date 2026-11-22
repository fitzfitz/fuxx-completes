//! `fuxx-completes why <line>` (`docs/design.md` §7, M1d §4.5): why a line ranks where it does.
//! Local only; it reads what `serve` reads and writes nothing.

use fuxx_completes_core::context::index::Tally;
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// What the core knows about one line, gathered by `Session::explain`.
pub struct Facts<'a> {
    pub line: &'a str,
    pub cwd: &'a Path,
    /// The cwd's repo key (`None`: not in a repo).
    pub repo: Option<PathBuf>,
    pub in_history: bool,
    /// Whether records could be matched at all (a secret exists and the history is keyed).
    pub keyed: bool,
    pub here: Option<Tally>,
    /// The repo's other folders, merged.
    pub in_repo: Option<Tally>,
    pub anywhere: Option<Tally>,
    /// `pnpm (package.json) in <folder>` when it is a project command.
    pub project: Option<String>,
    pub secret: bool,
    /// The dangerous part that must be typed, when it is dangerous.
    pub guarded: Option<&'a str>,
    pub cd_missing: bool,
    /// Unix seconds now.
    pub now: i64,
}

fn age(now: i64, then: i64) -> String {
    let s = (now - then).max(0);
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86_399 => format!("{} h ago", s / 3600),
        _ => format!("{} d ago", s / 86_400),
    }
}

fn tally(t: Option<Tally>, now: i64, none: &str) -> String {
    match t {
        Some(t) if t.ok + t.failed > 0 || t.last_time > 0 => format!(
            "ok {} · failed {} · newest {}",
            t.ok,
            t.failed,
            age(now, t.last_time)
        ),
        _ => none.to_string(),
    }
}

/// The step of the order (`docs/design.md` §5.4) the line is offered in.
fn step(f: &Facts) -> &'static str {
    let failed_only = |t: Option<Tally>| t.is_some_and(|t| t.ok == 0 && t.failed > 0);
    if f.secret {
        return "never shown";
    }
    if !f.in_history {
        return if f.project.is_some() {
            "a project command"
        } else {
            "never suggested (not in your history)"
        };
    }
    if f.guarded.is_some() {
        return "the rest of history, newest first (dangerous: no lift)";
    }
    if f.here.is_some() {
        return if failed_only(f.here) {
            "failed only here: ranks last"
        } else {
            "this folder's history"
        };
    }
    if f.in_repo.is_some_and(|t| t.ok > 0) {
        return "this repo's history";
    }
    if f.project.is_some() {
        return "a project command";
    }
    if failed_only(f.anywhere) {
        return "failed only: ranks last";
    }
    "the rest of history, newest first"
}

/// The explanation, one fact per line.
pub fn render(f: &Facts) -> String {
    let mut out = format!("fuxx-completes why: {}\n", f.line);
    let mut line = |label: &str, value: &str| {
        let _ = writeln!(out, "{label:<11}{value}");
    };
    line("folder", &f.cwd.display().to_string());
    line(
        "repo",
        &f.repo
            .as_ref()
            .map_or("none (not in a git repo)".into(), |r| {
                r.display().to_string()
            }),
    );
    line(
        "history",
        if f.in_history {
            "in your history"
        } else {
            "not in your history"
        },
    );
    if f.keyed {
        line("here", &tally(f.here, f.now, "no runs recorded here"));
        if f.repo.is_some() {
            line(
                "in repo",
                &tally(f.in_repo, f.now, "no runs in the repo's other folders"),
            );
        }
        line("anywhere", &tally(f.anywhere, f.now, "no runs recorded"));
    } else {
        line("records", "none to match (no context file yet)");
    }
    if let Some(p) = &f.project {
        line("project", p);
    }
    line("step", step(f));
    line(
        "follows",
        "not known here: each shell's core keeps what followed what in memory only",
    );
    if f.secret {
        line("flag", "never shown: looks like it holds a secret");
    }
    if let Some(g) = f.guarded {
        line("flag", &format!("guarded: shown once you type \"{g}\""));
    }
    if f.cd_missing {
        line("flag", "passed over: a cd to a folder that isn't here");
    }
    out
}
