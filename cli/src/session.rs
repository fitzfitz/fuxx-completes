//! One adapter's conversation with the core: the history it knows, the context files, and how
//! each message is answered.

use fuxx_completes_core::cd_target::Place;
use fuxx_completes_core::context::index::{Tally, NEUTRAL_STATUSES};
use fuxx_completes_core::context::secret::Secret;
use fuxx_completes_core::context::store::{ContextStore, CAP};
use fuxx_completes_core::context::Key;
use fuxx_completes_core::context::{data_dir_from_env, DataDir};
use fuxx_completes_core::history::{History, HistoryFormat};
use fuxx_completes_core::history_file::HistoryFile;
use fuxx_completes_core::projects::cache::ProjectCache;
use fuxx_completes_core::protocol::{DecodeError, Request, Response, PROTOCOL_VERSION};
use fuxx_completes_core::rank::{Ranking, Successors};
use fuxx_completes_core::repo::RepoCache;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A list to push once the pause is over (M1c): for the newest `suggest` only.
struct PendingList {
    id: u64,
    buffer: String,
    cwd: PathBuf,
    cdpath: Option<Vec<PathBuf>>,
    due: Instant,
}

/// The merged tallies of the cwd's repo (M1d §4.3), kept while the cwd and the records stay the
/// same.
struct RepoView {
    cwd: PathBuf,
    records: u64,
    tallies: Option<HashMap<Key, Tally>>,
}

/// Longest suggestion text sent back, in characters (same cap as the adapter's buffer).
const MAX_SUGGESTION_CHARS: usize = 1000;

pub struct Session {
    history: History,
    /// The core's own `$HOME`, for `cd ~/…` targets (`None` keeps them).
    home: Option<PathBuf>,
    /// Set by `hello`; without it (no path, unknown format, no hello yet) only `ran` commands count.
    file: Option<HistoryFile>,
    /// The data folder (`None`: no context at all).
    data: Option<DataDir>,
    /// Opened by `hello`.
    store: Option<ContextStore>,
    successors: Successors,
    last_ran: Option<String>,
    /// The adapter's `prev`: the command just run.
    previous: Option<String>,
    /// This core compacted its own context file once the history had loaded.
    compacted: bool,
    /// What the projects around each folder offer (M1b), memory only.
    projects: ProjectCache,
    /// The suggestion list's settings (M1c `listcfg`); size 0 = no list.
    list_size: u8,
    list_delay: Duration,
    list_empty: bool,
    pending: Option<PendingList>,
    /// Repo keys of folders, and the cwd's repo tallies (M1d), memory only.
    repos: RepoCache,
    repo_view: Option<RepoView>,
    /// The previous command failed (`prev`'s status: non-zero, not 130 or 143; M1d §4.4).
    previous_failed: bool,
    /// The fix for it, per folder and project count (detection may still be running).
    fix: Option<(PathBuf, usize, Option<String>)>,
    /// The previous command's exit status, when `prev` sent one.
    previous_status: i32,
    /// `why`'s session: never compacts (it writes nothing).
    read_only: bool,
}

impl Session {
    /// A session on the data folder `data` (`None`: no context), with `home` for `cd ~/…`.
    pub fn new(data: Option<DataDir>, home: Option<PathBuf>) -> Self {
        Session {
            history: History::new(),
            home,
            file: None,
            data,
            store: None,
            successors: Successors::new(),
            last_ran: None,
            previous: None,
            compacted: false,
            projects: ProjectCache::new(),
            list_size: 0,
            list_delay: Duration::from_secs(3),
            list_empty: true,
            pending: None,
            repos: RepoCache::new(),
            repo_view: None,
            previous_failed: false,
            fix: None,
            previous_status: 0,
            read_only: false,
        }
    }

    /// A session for `fuxx-completes why`: reads what `serve` reads, writes nothing.
    pub fn for_why(data: Option<DataDir>, home: Option<PathBuf>) -> Self {
        let mut s = Self::new(data, home);
        s.read_only = true;
        s
    }

    /// Why `line` ranks where it does in `cwd` (M1d §4.5), once the history has loaded (or
    /// `deadline` passed).
    pub fn explain(&mut self, line: &str, cwd: &Path, deadline: Instant) -> String {
        while !self.history_loaded() && Instant::now() < deadline {
            self.refresh();
            std::thread::sleep(Duration::from_millis(5));
        }
        self.refresh();
        self.update_repo(cwd);
        let home = self.home.as_deref();
        let detection = fuxx_completes_core::projects::detect::detect(
            cwd,
            home,
            Instant::now() + Duration::from_secs(1),
        );
        let project = detection
            .candidates
            .iter()
            .position(|c| c == line)
            .and_then(|i| detection.origins.get(i))
            .map(|o| {
                let folder = detection
                    .folders
                    .get(o.folder)
                    .map(|f| f.display().to_string());
                format!(
                    "{} ({}) in {}",
                    o.id,
                    o.file.unwrap_or("built in"),
                    folder.unwrap_or_default()
                )
            });
        let index = self.store.as_ref().map(ContextStore::index);
        let key = self.history.key_of(line).filter(|_| index.is_some());
        let place = Place {
            cwd,
            home,
            cdpath: None,
        };
        let reach = fuxx_completes_core::safety::danger_reach(line);
        let facts = crate::why::Facts {
            line,
            cwd,
            repo: self.repos.key(cwd, home),
            in_history: self.history.find(line).is_some(),
            keyed: key.is_some(),
            here: key.and_then(|k| index?.in_folder(cwd)?.get(&k).copied()),
            in_repo: key.and_then(|k| self.repo_view.as_ref()?.tallies.as_ref()?.get(&k).copied()),
            anywhere: key.and_then(|k| index?.anywhere(k)),
            project,
            secret: fuxx_completes_core::safety::looks_secret(line),
            guarded: reach.and_then(|r| line.get(..r)),
            cd_missing: fuxx_completes_core::cd_target::certainly_missing(line, &place),
            now: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64),
        };
        crate::why::render(&facts)
    }

    /// The data folder and `$HOME` from this process's environment (`main` only: tests pass
    /// their own folders, so they never touch the real ones).
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(PathBuf::from);
        Self::new(data_dir_from_env(), home)
    }

    /// True once the history file's first full read was applied (or there is no file).
    pub fn history_loaded(&self) -> bool {
        self.file.as_ref().is_none_or(HistoryFile::has_loaded)
    }

    /// Hello sets up the history file and the context files and replies `ready` whatever
    /// protocol version was asked (the adapter checks it). Suggest refreshes, then answers. Ran
    /// and prev record; Bye does nothing here (`serve` stops on it). Those three don't reply.
    pub fn handle(&mut self, req: Request) -> Option<Response> {
        match req {
            Request::Hello {
                history_format,
                history_path,
                ..
            } => {
                self.store = self
                    .data
                    .clone()
                    .map(|d| ContextStore::open(d, history_path.clone()));
                self.history
                    .set_secret(self.store.as_ref().and_then(ContextStore::secret));
                self.file = history_path
                    .zip(history_format.parse::<HistoryFormat>().ok())
                    .map(|(path, format)| HistoryFile::new(path, format));
                Some(Response::Ready {
                    protocol: PROTOCOL_VERSION,
                })
            }
            Request::Suggest {
                id,
                buffer,
                cwd,
                cdpath,
                ..
            } => {
                self.refresh();
                self.update_repo(&cwd);
                let place = Place {
                    cwd: &cwd,
                    home: self.home.as_deref(),
                    cdpath: cdpath.as_deref(),
                };
                let ranking = Ranking {
                    index: self.store.as_ref().map(ContextStore::index),
                    cwd: &cwd,
                    previous: self.previous.as_deref(),
                    successors: Some(&self.successors),
                    repo: self.repo_view.as_ref().and_then(|v| v.tallies.as_ref()),
                };
                // Longer than the adapter's own 1000-character buffer cap: drawing it stalls ZLE
                // (a 200 KB entry took about 200 ms), so answer "no suggestion".
                let projects = self.projects.candidates(&cwd, self.home.as_deref());
                let text = if buffer.is_empty() {
                    // The empty line right after a failure: the fix (M1d §4.4).
                    let failed = self.previous.as_deref().filter(|_| self.previous_failed);
                    cached_fix(
                        &mut self.fix,
                        failed,
                        self.previous_status,
                        &self.history,
                        &ranking,
                        projects,
                    )
                } else {
                    fuxx_autosuggestions::suggestion(
                        &self.history,
                        &buffer,
                        &place,
                        &ranking,
                        projects,
                    )
                    .filter(|t| sendable(t))
                    .map(str::to_owned)
                };
                let wants_list = self.list_size > 0 && (!buffer.is_empty() || self.list_empty);
                self.pending = wants_list.then(|| PendingList {
                    id,
                    buffer,
                    cwd,
                    cdpath,
                    due: Instant::now() + self.list_delay,
                });
                Some(Response::Suggestion { id, text })
            }
            Request::Ran {
                cwd,
                status,
                time,
                command,
            } => {
                if let Some(store) = &mut self.store {
                    let before = store.secret().cloned();
                    store.record(&command, &cwd, status, time);
                    sync_secret(&mut self.history, before, store);
                }
                if let Some(last) = &self.last_ran {
                    self.successors.record(last, &command);
                }
                self.last_ran = Some(command.clone());
                self.pending = None;
                self.history.note_ran(command);
                None
            }
            Request::Prev { command, status } => {
                self.previous = command;
                self.previous_failed =
                    status.is_some_and(|s| s != 0 && !NEUTRAL_STATUSES.contains(&s));
                self.fix = None;
                self.previous_status = status.unwrap_or(0);
                None
            }
            Request::ListConfig { size, delay, empty } => {
                self.list_size = size.min(5);
                self.list_delay = delay;
                self.list_empty = empty;
                if self.list_size == 0 {
                    self.pending = None;
                }
                None
            }
            Request::Bye => {
                self.pending = None;
                None
            }
        }
    }

    /// When the pending list is due, if there is one (the serve loop waits until then).
    pub fn next_deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|p| p.due)
    }

    /// The list for the newest request, once its pause is over: whole lines in the ghost text's
    /// order (M1c). `None` before the deadline, or when nothing matches.
    pub fn tick(&mut self, now: Instant) -> Option<Response> {
        if self.pending.as_ref()?.due > now {
            return None;
        }
        let p = self.pending.take()?;
        self.refresh();
        self.update_repo(&p.cwd);
        let place = Place {
            cwd: &p.cwd,
            home: self.home.as_deref(),
            cdpath: p.cdpath.as_deref(),
        };
        let ranking = Ranking {
            index: self.store.as_ref().map(ContextStore::index),
            cwd: &p.cwd,
            previous: self.previous.as_deref(),
            successors: Some(&self.successors),
            repo: self.repo_view.as_ref().and_then(|v| v.tallies.as_ref()),
        };
        let projects = self.projects.candidates(&p.cwd, self.home.as_deref());
        let mut items: Vec<String> = fuxx_autosuggestions::suggestion_list(
            &self.history,
            &p.buffer,
            &place,
            &ranking,
            projects,
            self.list_size as usize,
        )
        .into_iter()
        .filter(|t| sendable(t))
        .map(str::to_owned)
        .collect();
        if p.buffer.is_empty() {
            let failed = self.previous.as_deref().filter(|_| self.previous_failed);
            if let Some(fix) = cached_fix(
                &mut self.fix,
                failed,
                self.previous_status,
                &self.history,
                &ranking,
                projects,
            ) {
                items.retain(|i| *i != fix);
                items.insert(0, fix);
                items.truncate(self.list_size as usize);
            }
        }
        (!items.is_empty()).then_some(Response::List { id: p.id, items })
    }

    /// The cwd's repo tallies, looked up again when the cwd or the records changed.
    fn update_repo(&mut self, cwd: &Path) {
        let Some(index) = self.store.as_ref().map(ContextStore::index) else {
            self.repo_view = None;
            return;
        };
        let records = index.records();
        if self
            .repo_view
            .as_ref()
            .is_some_and(|v| v.cwd == cwd && v.records == records)
        {
            return;
        }
        let tallies = self.repos.tallies(index, cwd, self.home.as_deref());
        self.repo_view = Some(RepoView {
            cwd: cwd.to_path_buf(),
            records,
            tallies,
        });
    }

    /// Before an answer: the context files (a `forget`, a secret another core made, new
    /// records), the history file, and this core's one compaction once the history has loaded
    /// (again whenever its own file is over the cap).
    fn refresh(&mut self) {
        if let Some(store) = &mut self.store {
            let before = store.secret().cloned();
            store.refresh();
            sync_secret(&mut self.history, before, store);
        }
        if let Some(file) = &mut self.file {
            file.refresh(&mut self.history);
        }
        if let Some(store) = &mut self.store {
            let loaded = self.file.as_ref().is_some_and(HistoryFile::has_loaded);
            if !self.read_only
                && loaded
                && store.secret().is_some()
                && (!self.compacted || store.own_file_len() > CAP)
            {
                store.compact_in_background(self.history.keys().collect());
                self.compacted = true;
            }
        }
    }

    /// A suggest whose id parsed still gets an empty answer, so the adapter's one-in-flight
    /// slot clears; every other bad line is dropped.
    pub fn handle_error(&mut self, err: DecodeError) -> Option<Response> {
        match err {
            DecodeError::Malformed { id: Some(id) } => {
                Some(Response::Suggestion { id, text: None })
            }
            _ => None,
        }
    }
}

/// Whether a line can go back on the wire: at most 1000 characters (drawing more stalls ZLE) and
/// no `US`/`RS` byte.
fn sendable(t: &str) -> bool {
    t.chars().take(MAX_SUGGESTION_CHARS + 1).count() <= MAX_SUGGESTION_CHARS
        && !t.bytes().any(|b| b == 0x1f || b == 0x1e)
}

/// The fix for `failed` (M1d §4.4), worked out once per folder and project count.
fn cached_fix(
    cache: &mut Option<(PathBuf, usize, Option<String>)>,
    failed: Option<&str>,
    status: i32,
    history: &History,
    ranking: &Ranking,
    projects: &[String],
) -> Option<String> {
    let failed = failed?;
    if let Some((cwd, n, fix)) = cache.as_ref() {
        if cwd == ranking.cwd && *n == projects.len() {
            return fix.clone();
        }
    }
    let fix = fuxx_autosuggestions::fix_after_failure(history, failed, status, ranking, projects)
        .filter(|f| sendable(f));
    *cache = Some((ranking.cwd.to_path_buf(), projects.len(), fix.clone()));
    fix
}

/// Keys the history again when the store's secret changed (made, replaced or forgotten).
fn sync_secret(history: &mut History, before: Option<Secret>, store: &ContextStore) {
    if store.secret() != before.as_ref() {
        history.set_secret(store.secret());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fuxx_completes_core::protocol::{Mode, PROTOCOL_VERSION};
    use std::path::{Path, PathBuf};

    fn hello(version: u32, path: Option<&str>) -> Request {
        Request::Hello {
            protocol: version,
            shell: "zsh".into(),
            shell_version: "5.9".into(),
            history_format: "zsh".into(),
            history_path: path.map(PathBuf::from),
            libraries: vec!["autosuggestions".into()],
        }
    }

    fn suggest(id: u64, buffer: &str) -> Request {
        Request::Suggest {
            id,
            mode: Mode::Async,
            cursor: buffer.chars().count(),
            cwd: PathBuf::from("/"),
            buffer: buffer.into(),
            cdpath: None,
        }
    }

    #[test]
    fn hello_replies_ready_1() {
        let mut s = Session::new(None, None);
        assert_eq!(
            s.handle(hello(1, None)),
            Some(Response::Ready {
                protocol: PROTOCOL_VERSION
            })
        );
        // Whatever version was asked, the reply states ours.
        assert_eq!(
            s.handle(hello(7, None)),
            Some(Response::Ready { protocol: 1 })
        );
    }

    #[test]
    fn suggest_before_hello_answers_none() {
        let mut s = Session::new(None, None);
        assert_eq!(
            s.handle(suggest(1, "git")),
            Some(Response::Suggestion { id: 1, text: None })
        );
    }

    #[test]
    fn ran_command_is_suggested() {
        let mut s = Session::new(None, None);
        s.handle(hello(1, None));
        let ran = Request::Ran {
            cwd: PathBuf::from("/"),
            status: 0,
            time: 0,
            command: "make test".into(),
        };
        assert_eq!(s.handle(ran), None);
        assert_eq!(
            s.handle(suggest(2, "make")),
            Some(Response::Suggestion {
                id: 2,
                text: Some(" test".into())
            })
        );
    }

    #[test]
    fn suggestion_text_over_1000_chars_is_dropped() {
        let ran = |command: String| Request::Ran {
            cwd: PathBuf::from("/"),
            status: 0,
            time: 0,
            command,
        };
        let mut s = Session::new(None, None);
        s.handle(hello(1, None));
        // Exactly 1000 characters after the buffer still counts.
        s.handle(ran(format!("make {}", "x".repeat(999))));
        assert_eq!(
            s.handle(suggest(1, "make")),
            Some(Response::Suggestion {
                id: 1,
                text: Some(format!(" {}", "x".repeat(999)))
            })
        );
        // One more character is over the cap: no suggestion (it would stall ZLE while drawn).
        s.handle(ran(format!("make {}", "y".repeat(1000))));
        assert_eq!(
            s.handle(suggest(2, "make")),
            Some(Response::Suggestion { id: 2, text: None })
        );
        // Characters, not bytes: 1000 two-byte characters pass.
        s.handle(ran(format!("run {}", "é".repeat(999))));
        assert_eq!(
            s.handle(suggest(3, "run")),
            Some(Response::Suggestion {
                id: 3,
                text: Some(format!(" {}", "é".repeat(999)))
            })
        );
    }

    fn ran_in(cwd: &Path, command: &str) -> Request {
        Request::Ran {
            cwd: cwd.into(),
            status: 0,
            time: 0,
            command: command.into(),
        }
    }

    fn suggest_in(id: u64, buffer: &str, cwd: &Path, cdpath: Option<Vec<PathBuf>>) -> Request {
        Request::Suggest {
            id,
            mode: Mode::Async,
            cursor: buffer.chars().count(),
            cwd: cwd.into(),
            buffer: buffer.into(),
            cdpath,
        }
    }

    fn text(r: Option<Response>) -> Option<String> {
        match r {
            Some(Response::Suggestion { text, .. }) => text,
            other => panic!("not a suggestion: {other:?}"),
        }
    }

    /// The user's report: in Kerja, `cd ` suggested `fuxx-cli`, which is in Kerja/proj.
    #[test]
    fn cd_suggestion_is_checked_against_the_request_cwd() {
        let dir = tempfile::tempdir().unwrap();
        let kerja = std::fs::canonicalize(dir.path()).unwrap();
        let proj = kerja.join("proj");
        std::fs::create_dir_all(proj.join("fuxx-cli")).unwrap();
        let mut s = Session::new(None, None);
        s.handle(hello(1, None));
        s.handle(ran_in(&kerja, "cd proj"));
        s.handle(ran_in(&proj, "cd fuxx-cli"));
        let none = Some(vec![]);
        assert_eq!(
            text(s.handle(suggest_in(1, "cd ", &kerja, none.clone()))),
            Some("proj".into())
        );
        assert_eq!(
            text(s.handle(suggest_in(2, "cd ", &proj, none))),
            Some("fuxx-cli".into())
        );
    }

    #[test]
    fn cdpath_from_the_request_is_searched() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(root.join("lib/sub")).unwrap();
        std::fs::create_dir_all(root.join("here")).unwrap();
        let mut s = Session::new(None, None);
        s.handle(hello(1, None));
        s.handle(ran_in(&root, "cd sub"));
        let here = root.join("here");
        // Known and empty: `sub` isn't reachable from `here`.
        assert_eq!(
            text(s.handle(suggest_in(1, "cd s", &here, Some(vec![])))),
            None
        );
        // Through cdpath it is.
        assert_eq!(
            text(s.handle(suggest_in(2, "cd s", &here, Some(vec![root.join("lib")])))),
            Some("ub".into())
        );
        // Unknown (an adapter that doesn't send it): kept.
        assert_eq!(
            text(s.handle(suggest_in(3, "cd s", &here, None))),
            Some("ub".into())
        );
    }

    #[test]
    fn tilde_uses_the_core_home() {
        let dir = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(home.join("docs")).unwrap();
        let mut s = Session::new(None, None);
        s.home = Some(home.clone());
        s.handle(hello(1, None));
        s.handle(ran_in(&home, "cd ~/docs"));
        s.handle(ran_in(&home, "cd ~/nope"));
        assert_eq!(
            text(s.handle(suggest_in(1, "cd ~/", Path::new("/"), Some(vec![])))),
            Some("docs".into())
        );
    }

    #[test]
    fn malformed_suggest_gets_empty_reply() {
        let mut s = Session::new(None, None);
        assert_eq!(
            s.handle_error(DecodeError::Malformed { id: Some(5) }),
            Some(Response::Suggestion { id: 5, text: None })
        );
        assert_eq!(s.handle_error(DecodeError::Malformed { id: None }), None);
    }

    #[test]
    fn unknown_kind_gets_no_reply() {
        let mut s = Session::new(None, None);
        assert_eq!(s.handle_error(DecodeError::UnknownKind("zzz".into())), None);
    }

    // ---- M1: context ----

    use fuxx_completes_core::context::record::Record;
    use fuxx_completes_core::context::secret::Secret;
    use fuxx_completes_core::context::store::{append, forget, own_file_id};
    use fuxx_completes_core::context::{DataDir, DataSource, Key};
    use std::time::{Duration, Instant};

    struct Ctx {
        _dir: tempfile::TempDir,
        root: PathBuf,
        hist: PathBuf,
        p: PathBuf,
    }

    impl Ctx {
        /// A data folder, a history file holding `entries` (oldest first) and a folder `p`.
        fn new(entries: &[&str]) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let base = std::fs::canonicalize(dir.path()).unwrap();
            let hist = base.join(".zsh_history");
            let mut text = String::new();
            for e in entries {
                text.push_str(e);
                text.push('\n');
            }
            std::fs::write(&hist, text).unwrap();
            let p = base.join("p");
            std::fs::create_dir_all(&p).unwrap();
            Ctx {
                root: base.join("data"),
                hist,
                p,
                _dir: dir,
            }
        }
        fn data(&self) -> Option<DataDir> {
            Some(DataDir {
                root: self.root.clone(),
                source: DataSource::Xdg,
            })
        }
        fn session(&self) -> Session {
            let mut s = Session::new(self.data(), None);
            s.handle(hello(1, Some(self.hist.to_str().unwrap())));
            s
        }
        fn own(&self, secret: &Secret) -> String {
            own_file_id(secret, &self.hist)
        }
    }

    #[test]
    fn bash_hello_serves_a_bash_history() {
        let dir = tempfile::tempdir().unwrap();
        let hist = dir.path().join(".bash_history");
        std::fs::write(&hist, "#1700000000\ngit status\n#1700000001\nls\n").unwrap();
        let mut s = Session::new(None, None);
        s.handle(Request::Hello {
            protocol: 1,
            shell: "bash".into(),
            shell_version: "5.3.0(1)-release".into(),
            history_format: "bash".into(),
            history_path: Some(hist),
            libraries: vec!["autosuggestions".into()],
        });
        let got = ask_until(&mut s, "git s", Path::new("/"), Some("tatus"));
        assert_eq!(got.as_deref(), Some("tatus"));
    }

    /// Asks until the answer is `want` (the history loads in the background), up to 2 s.
    fn ask_until(s: &mut Session, buffer: &str, cwd: &Path, want: Option<&str>) -> Option<String> {
        let end = Instant::now() + Duration::from_secs(2);
        let mut id = 100;
        loop {
            id += 1;
            let got = text(s.handle(suggest_in(id, buffer, cwd, None)));
            if got.as_deref() == want || Instant::now() >= end {
                return got;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn ran_status(cwd: &Path, command: &str, status: i32) -> Request {
        Request::Ran {
            cwd: cwd.into(),
            status,
            time: 1,
            command: command.into(),
        }
    }

    #[test]
    fn context_survives_a_restart() {
        let c = Ctx::new(&["make test", "make build"]);
        let mut s1 = c.session();
        assert_eq!(
            ask_until(&mut s1, "make ", &c.p, Some("build")).as_deref(),
            Some("build")
        );
        s1.handle(ran_in(&c.p, "make test"));
        s1.handle(ran_in(&c.p, "make test"));
        drop(s1);
        let mut s2 = c.session();
        assert_eq!(
            ask_until(&mut s2, "make ", &c.p, Some("test")).as_deref(),
            Some("test")
        );
        // Elsewhere the newest wins.
        assert_eq!(
            ask_until(&mut s2, "make ", Path::new("/"), Some("build")).as_deref(),
            Some("build")
        );
    }

    #[test]
    fn prev_message_drives_follows() {
        let mut s = Session::new(None, None);
        s.handle(hello(1, None));
        s.handle(ran_in(Path::new("/"), "git add ."));
        s.handle(ran_in(Path::new("/"), "git commit -m x"));
        s.handle(ran_in(Path::new("/"), "git checkout main"));
        assert_eq!(
            text(s.handle(suggest(1, "git c"))),
            Some("heckout main".into())
        );
        assert_eq!(
            s.handle(Request::Prev {
                command: Some("git add .".into()),
                status: None,
            }),
            None
        );
        assert_eq!(
            text(s.handle(suggest(2, "git c"))),
            Some("ommit -m x".into())
        );
        s.handle(Request::Prev {
            command: None,
            status: None,
        });
        assert_eq!(
            text(s.handle(suggest(3, "git c"))),
            Some("heckout main".into())
        );
    }

    #[test]
    fn forget_while_serving_drops_context() {
        let c = Ctx::new(&["make test", "make build"]);
        let secret = Secret::load_or_create(&c.root).unwrap();
        let r = Record {
            key: secret.key_of("make test"),
            folder: c.p.clone(),
            status: 0,
            time: 1,
        };
        append(&c.root, &c.own(&secret), &[r]).unwrap();
        let mut s = c.session();
        assert_eq!(
            ask_until(&mut s, "make ", &c.p, Some("test")).as_deref(),
            Some("test")
        );
        assert!(forget(&c.root, false).unwrap());
        assert_eq!(
            text(s.handle(suggest_in(1, "make ", &c.p, None))),
            Some("build".into())
        );
        let later = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            + 5;
        s.handle(Request::Ran {
            cwd: c.p.clone(),
            status: 0,
            time: later,
            command: "make zzz".into(),
        });
        let new = Secret::load(&c.root).unwrap().expect("a new key");
        assert!(new != secret);
        let file = std::fs::read(c.root.join("context").join(c.own(&new))).unwrap();
        assert_eq!(file.iter().filter(|&&b| b == b'\n').count(), 1);
    }

    #[test]
    fn unwritable_data_folder_still_suggests() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        std::fs::write(&root, b"a file where the folder should be").unwrap();
        let hist = dir.path().join(".zsh_history");
        std::fs::write(&hist, "ls\n").unwrap();
        let mut s = Session::new(
            Some(DataDir {
                root,
                source: DataSource::Xdg,
            }),
            None,
        );
        s.handle(hello(1, Some(hist.to_str().unwrap())));
        s.handle(ran_status(Path::new("/"), "make test", 2));
        assert_eq!(text(s.handle(suggest(1, "make"))), Some(" test".into()));
    }

    #[test]
    fn first_full_read_triggers_one_compaction() {
        let c = Ctx::new(&["make test"]);
        let secret = Secret::load_or_create(&c.root).unwrap();
        let rec = |cmd: &str| Record {
            key: secret.key_of(cmd),
            folder: c.p.clone(),
            status: 0,
            time: 1,
        };
        let own = c.own(&secret);
        append(&c.root, &own, &[rec("gone"), rec("make test")]).unwrap();
        let mut s = c.session();
        let path = c.root.join("context").join(&own);
        let gone: Key = secret.key_of("gone");
        let end = Instant::now() + Duration::from_secs(2);
        let mut id = 0;
        loop {
            id += 1;
            s.handle(suggest_in(id, "make", &c.p, None));
            let bytes = std::fs::read(&path).unwrap();
            if !String::from_utf8_lossy(&bytes).contains(&gone.to_hex()) {
                assert!(
                    String::from_utf8_lossy(&bytes).contains(&secret.key_of("make test").to_hex())
                );
                break;
            }
            assert!(Instant::now() < end, "never compacted");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_missing_history_file_never_triggers_compaction() {
        let c = Ctx::new(&[]);
        std::fs::remove_file(&c.hist).unwrap();
        let secret = Secret::load_or_create(&c.root).unwrap();
        let r = Record {
            key: secret.key_of("kept"),
            folder: c.p.clone(),
            status: 0,
            time: 1,
        };
        let own = c.own(&secret);
        append(&c.root, &own, &[r]).unwrap();
        let mut s = c.session();
        for id in 0..40 {
            s.handle(suggest_in(id, "k", &c.p, None));
            std::thread::sleep(Duration::from_millis(10));
        }
        let bytes = std::fs::read(c.root.join("context").join(&own)).unwrap();
        assert_eq!(
            bytes.iter().filter(|&&b| b == b'\n').count(),
            1,
            "the record was compacted away"
        );
    }

    #[test]
    fn a_cold_project_folder_fills_in_on_a_later_request() {
        let dir = tempfile::tempdir().unwrap();
        let repo = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(repo.join(".git"), "").unwrap();
        std::fs::write(repo.join("Cargo.toml"), "").unwrap();
        let mut s = Session::new(None, None);
        s.handle(hello(1, None));
        assert_eq!(text(s.handle(suggest_in(1, "cargo t", &repo, None))), None);
        assert_eq!(
            ask_until(&mut s, "cargo t", &repo, Some("est")).as_deref(),
            Some("est")
        );
    }

    // ---- M1c: the list ----

    fn listcfg(size: u8, ms: u64, empty: bool) -> Request {
        Request::ListConfig {
            size,
            delay: Duration::from_millis(ms),
            empty,
        }
    }

    fn lister(entries: &[&str]) -> Session {
        let mut s = Session::new(None, None);
        s.handle(hello(1, None));
        for e in entries {
            s.handle(ran_in(Path::new("/"), e));
        }
        s
    }

    #[test]
    fn a_list_is_due_after_the_delay() {
        let mut s = lister(&["git status", "git stash"]);
        s.handle(listcfg(5, 100, true));
        s.handle(suggest(1, "git s"));
        assert!(s.next_deadline().is_some());
        assert_eq!(s.tick(Instant::now() + Duration::from_millis(50)), None);
        assert_eq!(
            s.tick(Instant::now() + Duration::from_millis(100)),
            Some(Response::List {
                id: 1,
                items: vec!["git stash".into(), "git status".into()]
            })
        );
        assert!(s.next_deadline().is_none(), "one list per request");
    }

    #[test]
    fn a_newer_suggest_rearms_the_list() {
        let mut s = lister(&["git status"]);
        s.handle(listcfg(5, 100, true));
        s.handle(suggest(1, "git"));
        s.handle(suggest(2, "git s"));
        match s.tick(Instant::now() + Duration::from_millis(200)) {
            Some(Response::List { id, .. }) => assert_eq!(id, 2),
            other => panic!("{other:?}"),
        }
        assert_eq!(s.tick(Instant::now() + Duration::from_secs(1)), None);
    }

    #[test]
    fn no_list_without_config_or_with_size_0() {
        let mut s = lister(&["git status"]);
        s.handle(suggest(1, "git"));
        assert!(s.next_deadline().is_none());
        s.handle(listcfg(0, 100, true));
        s.handle(suggest(2, "git"));
        assert!(s.next_deadline().is_none());
        s.handle(listcfg(5, 100, true));
        s.handle(suggest(3, "git"));
        s.handle(Request::Ran {
            cwd: "/".into(),
            status: 0,
            time: 1,
            command: "ls".into(),
        });
        assert!(
            s.next_deadline().is_none(),
            "a command ran: the line is gone"
        );
    }

    #[test]
    fn empty_line_list_only_when_enabled() {
        let mut s = lister(&["git status"]);
        s.handle(listcfg(5, 10, false));
        assert_eq!(
            s.handle(suggest(1, "")),
            Some(Response::Suggestion { id: 1, text: None })
        );
        assert!(s.next_deadline().is_none());
        s.handle(listcfg(5, 10, true));
        s.handle(suggest(2, ""));
        match s.tick(Instant::now() + Duration::from_millis(20)) {
            Some(Response::List { id: 2, items }) => assert_eq!(items, ["git status"]),
            other => panic!("{other:?}"),
        }
    }

    /// M1d §4.3: a worktree learns from the main checkout's subfolders.
    #[test]
    fn a_worktree_learns_from_the_main_checkout() {
        let c = Ctx::new(&["make deploy", "make lint"]);
        let base = c.p.clone();
        let main = base.join("main");
        let wt = base.join("wt");
        let admin = main.join(".git/worktrees/wt");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::create_dir_all(main.join("sub")).unwrap();
        std::fs::create_dir_all(&wt).unwrap();
        std::fs::write(
            admin.join("gitdir"),
            format!("{}\n", wt.join(".git").display()),
        )
        .unwrap();
        std::fs::write(admin.join("commondir"), "../..\n").unwrap();
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
        let mut s = c.session();
        assert_eq!(
            ask_until(&mut s, "make ", &wt, Some("lint")).as_deref(),
            Some("lint")
        );
        s.handle(ran_in(&main.join("sub"), "make deploy"));
        // A newer run outside the repo.
        s.handle(ran_in(Path::new("/"), "make lint"));
        assert_eq!(
            ask_until(&mut s, "make ", &wt, Some("deploy")).as_deref(),
            Some("deploy")
        );
        // Outside the repo the newest wins.
        assert_eq!(
            ask_until(&mut s, "make ", &base, Some("lint")).as_deref(),
            Some("lint")
        );
    }

    fn prev(command: &str, status: Option<i32>) -> Request {
        Request::Prev {
            command: (!command.is_empty()).then(|| command.to_string()),
            status,
        }
    }

    /// A session where `ls` succeeded in `p` and `lss` then failed with `status`.
    fn after_lss(c: &Ctx, status: Option<i32>) -> Session {
        let mut s = c.session();
        ask_until(&mut s, "l", &c.p, Some("s"));
        s.handle(ran_in(&c.p, "ls"));
        s.handle(prev("lss", status));
        s
    }

    /// Review Focus 4: no fix after a success, ⌃C's 130, SIGTERM's 143 or an unknown status.
    #[test]
    fn no_fix_after_success_or_interrupt() {
        let c = Ctx::new(&["ls"]);
        for status in [Some(0), Some(130), Some(143), None] {
            let mut s = after_lss(&c, status);
            assert_eq!(
                text(s.handle(suggest_in(9, "", &c.p, None))),
                None,
                "{status:?}"
            );
        }
        let mut s = after_lss(&c, Some(127));
        s.handle(prev("", Some(1)));
        assert_eq!(
            text(s.handle(suggest_in(9, "", &c.p, None))),
            None,
            "unknown command"
        );
    }

    /// M1d §4.4: the fix is the empty line's ghost text, and item 1 of its list.
    #[test]
    fn fix_is_ghost_text_and_list_item_one() {
        let c = Ctx::new(&["ls", "git status"]);
        let mut s = after_lss(&c, Some(127));
        assert_eq!(
            text(s.handle(suggest_in(9, "", &c.p, None))).as_deref(),
            Some("ls")
        );
        // Typed text is not the empty line: no fix.
        assert_eq!(text(s.handle(suggest_in(10, "x", &c.p, None))), None);
        s.handle(listcfg(5, 0, true));
        s.handle(suggest_in(11, "", &c.p, None));
        match s.tick(Instant::now() + Duration::from_millis(10)) {
            Some(Response::List { items, .. }) => {
                assert_eq!(items[0], "ls");
                assert_eq!(items.iter().filter(|i| *i == "ls").count(), 1);
            }
            other => panic!("{other:?}"),
        }
    }
}
