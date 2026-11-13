//! One adapter's conversation with the core: the history it knows and how each message is answered.

use fuxx_completes_core::cd_target::Place;
use fuxx_completes_core::history::{History, HistoryFormat};
use fuxx_completes_core::history_file::HistoryFile;
use fuxx_completes_core::protocol::{DecodeError, Request, Response, PROTOCOL_VERSION};
use std::path::PathBuf;

/// Longest suggestion text sent back, in characters (same cap as the adapter's buffer).
const MAX_SUGGESTION_CHARS: usize = 1000;

pub struct Session {
    history: History,
    /// The core's own `$HOME`, for `cd ~/…` targets (`None` keeps them).
    home: Option<PathBuf>,
    /// Set by `hello`; without it (no path, unknown format, no hello yet) only `ran` commands count.
    file: Option<HistoryFile>,
}

impl Session {
    pub fn new() -> Self {
        Session {
            history: History::new(),
            home: std::env::var_os("HOME")
                .filter(|h| !h.is_empty())
                .map(PathBuf::from),
            file: None,
        }
    }

    /// Hello sets up the history file and replies `ready` whatever protocol version was asked
    /// (the adapter checks it). Suggest refreshes, then answers. Ran records; Bye does nothing
    /// here (`serve` stops on it). Neither of those two replies.
    pub fn handle(&mut self, req: Request) -> Option<Response> {
        match req {
            Request::Hello {
                history_format,
                history_path,
                ..
            } => {
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
                if let Some(file) = &mut self.file {
                    file.refresh(&mut self.history);
                }
                // Longer than the adapter's own 1000-character buffer cap: drawing it stalls ZLE
                // (a 200 KB entry took about 200 ms), so answer "no suggestion".
                let place = Place {
                    cwd: &cwd,
                    home: self.home.as_deref(),
                    cdpath: cdpath.as_deref(),
                };
                let text = fuxx_autosuggestions::suggestion(&self.history, &buffer, &place)
                    .filter(|t| {
                        t.chars().take(MAX_SUGGESTION_CHARS + 1).count() <= MAX_SUGGESTION_CHARS
                    })
                    .map(str::to_owned);
                Some(Response::Suggestion { id, text })
            }
            Request::Ran { command, .. } => {
                self.history.record_session(command);
                None
            }
            Request::Bye => None,
        }
    }

    /// A `suggest` whose id parsed still gets an empty answer, so the adapter's one-in-flight
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

impl Default for Session {
    fn default() -> Self {
        Self::new()
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
        let mut s = Session::new();
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
        let mut s = Session::new();
        assert_eq!(
            s.handle(suggest(1, "git")),
            Some(Response::Suggestion { id: 1, text: None })
        );
    }

    #[test]
    fn ran_command_is_suggested() {
        let mut s = Session::new();
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
        let mut s = Session::new();
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
        let mut s = Session::new();
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
        let mut s = Session::new();
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
        let mut s = Session::new();
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
        let mut s = Session::new();
        assert_eq!(
            s.handle_error(DecodeError::Malformed { id: Some(5) }),
            Some(Response::Suggestion { id: 5, text: None })
        );
        assert_eq!(s.handle_error(DecodeError::Malformed { id: None }), None);
    }

    #[test]
    fn unknown_kind_gets_no_reply() {
        let mut s = Session::new();
        assert_eq!(s.handle_error(DecodeError::UnknownKind("zzz".into())), None);
    }
}
