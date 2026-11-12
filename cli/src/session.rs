//! One adapter's conversation with the core: the history it knows and how each message is answered.

use fuxx_completes_core::history::{History, HistoryFormat};
use fuxx_completes_core::history_file::HistoryFile;
use fuxx_completes_core::protocol::{DecodeError, Request, Response, PROTOCOL_VERSION};

pub struct Session {
    history: History,
    /// Set by `hello`; without it (no path, unknown format, no hello yet) only `ran` commands count.
    file: Option<HistoryFile>,
}

impl Session {
    pub fn new() -> Self {
        Session {
            history: History::new(),
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
            Request::Suggest { id, buffer, .. } => {
                if let Some(file) = &mut self.file {
                    file.refresh(&mut self.history);
                }
                let text =
                    fuxx_autosuggestions::suggestion(&self.history, &buffer).map(str::to_owned);
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
    use std::path::PathBuf;

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
