//! Wire format 1: the messages between a shell adapter and the core (`docs/design.md` §5.5).
//!
//! One message per line. Fields are separated by `US` (0x1F); the line ends with LF. Inside a
//! field a newline is written as `RS` (0x1E). A field never otherwise contains `US` or `RS`.

use std::path::PathBuf;

/// The protocol version this crate speaks.
pub const PROTOCOL_VERSION: u32 = 1;

const US: u8 = 0x1f;
const RS: u8 = 0x1e;
const LF: u8 = b'\n';

/// Whether the adapter waits for the answer (`Sync`) or draws it when it arrives (`Async`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Async,
    Sync,
}

/// A message from an adapter to the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Hello {
        protocol: u32,
        shell: String,
        shell_version: String,
        history_format: String,
        history_path: Option<PathBuf>,
        libraries: Vec<String>,
    },
    Suggest {
        id: u64,
        mode: Mode,
        /// In code points.
        cursor: usize,
        cwd: PathBuf,
        buffer: String,
    },
    Ran {
        cwd: PathBuf,
        status: i32,
        /// Unix seconds.
        time: i64,
        command: String,
    },
    Bye,
}

/// A message from the core to an adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Ready {
        protocol: u32,
    },
    /// `text` is what to show after the buffer; `None` means no suggestion.
    Suggestion {
        id: u64,
        text: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The first field is not a message kind we know (callers ignore these).
    UnknownKind(String),
    /// A known kind with bad fields. For `suggest`, `id` is set when the id itself parsed, so
    /// the caller can still answer it with an empty suggestion.
    Malformed { id: Option<u64> },
}

/// Decodes one request line (without its LF).
pub fn decode_request(line: &[u8]) -> Result<Request, DecodeError> {
    let fields: Vec<&[u8]> = line.split(|&b| b == US).collect();
    match fields[0] {
        b"hello" => decode_hello(&fields),
        b"suggest" => decode_suggest(&fields),
        b"ran" => decode_ran(&fields),
        b"bye" if fields.len() == 1 => Ok(Request::Bye),
        b"bye" => Err(DecodeError::Malformed { id: None }),
        other => Err(unknown(other)),
    }
}

/// Encodes a request as one line ending with LF. A field holding `US` or `RS` is the caller's bug.
pub fn encode_request(req: &Request) -> Vec<u8> {
    match req {
        Request::Hello {
            protocol,
            shell,
            shell_version,
            history_format,
            history_path,
            libraries,
        } => {
            let path = history_path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned());
            join(&[
                "hello",
                &protocol.to_string(),
                shell,
                shell_version,
                history_format,
                path.as_deref().unwrap_or(""),
                &libraries.join(","),
            ])
        }
        Request::Suggest {
            id,
            mode,
            cursor,
            cwd,
            buffer,
        } => join(&[
            "suggest",
            &id.to_string(),
            match mode {
                Mode::Async => "async",
                Mode::Sync => "sync",
            },
            &cursor.to_string(),
            &cwd.to_string_lossy(),
            buffer,
        ]),
        Request::Ran {
            cwd,
            status,
            time,
            command,
        } => join(&[
            "ran",
            &cwd.to_string_lossy(),
            &status.to_string(),
            &time.to_string(),
            command,
        ]),
        Request::Bye => join(&["bye"]),
    }
}

/// Decodes one response line (without its LF).
pub fn decode_response(line: &[u8]) -> Result<Response, DecodeError> {
    let fields: Vec<&[u8]> = line.split(|&b| b == US).collect();
    let bad = |id| DecodeError::Malformed { id };
    match fields[0] {
        b"ready" => {
            let [_, protocol] = fields[..] else {
                return Err(bad(None));
            };
            Ok(Response::Ready {
                protocol: parse(protocol).ok_or(bad(None))?,
            })
        }
        b"suggestion" => {
            let [_, id, text] = fields[..] else {
                return Err(bad(None));
            };
            let id: u64 = parse(id).ok_or(bad(None))?;
            let text = field(text).ok_or(bad(Some(id)))?;
            Ok(Response::Suggestion {
                id,
                text: (!text.is_empty()).then_some(text),
            })
        }
        other => Err(unknown(other)),
    }
}

/// Encodes a response as one line ending with LF. Suggestion text that holds `US` or `RS` can't
/// be sent faithfully, so it goes out as "no suggestion".
pub fn encode_response(resp: &Response) -> Vec<u8> {
    match resp {
        Response::Ready { protocol } => join(&["ready", &protocol.to_string()]),
        Response::Suggestion { id, text } => {
            let text = text
                .as_deref()
                .filter(|t| !t.bytes().any(|b| b == US || b == RS))
                .unwrap_or("");
            join(&["suggestion", &id.to_string(), text])
        }
    }
}

fn decode_hello(fields: &[&[u8]]) -> Result<Request, DecodeError> {
    let bad = DecodeError::Malformed { id: None };
    let [_, protocol, shell, version, format, path, libs] = fields[..] else {
        return Err(bad);
    };
    let build = || {
        let path = field(path)?;
        let libs = field(libs)?;
        Some(Request::Hello {
            protocol: parse(protocol)?,
            shell: field(shell)?,
            shell_version: field(version)?,
            history_format: field(format)?,
            history_path: (!path.is_empty()).then(|| PathBuf::from(path)),
            libraries: if libs.is_empty() {
                Vec::new()
            } else {
                libs.split(',').map(String::from).collect()
            },
        })
    };
    build().ok_or(bad)
}

fn decode_suggest(fields: &[&[u8]]) -> Result<Request, DecodeError> {
    let [_, id, mode, cursor, cwd, buffer] = fields[..] else {
        return Err(DecodeError::Malformed {
            id: fields.get(1).and_then(|f| parse(f)),
        });
    };
    let id: u64 = parse(id).ok_or(DecodeError::Malformed { id: None })?;
    let build = || {
        Some(Request::Suggest {
            id,
            mode: match mode {
                b"async" => Mode::Async,
                b"sync" => Mode::Sync,
                _ => return None,
            },
            cursor: parse(cursor)?,
            cwd: PathBuf::from(field(cwd)?),
            buffer: field(buffer)?,
        })
    };
    build().ok_or(DecodeError::Malformed { id: Some(id) })
}

fn decode_ran(fields: &[&[u8]]) -> Result<Request, DecodeError> {
    let bad = DecodeError::Malformed { id: None };
    let [_, cwd, status, time, command] = fields[..] else {
        return Err(bad);
    };
    let build = || {
        Some(Request::Ran {
            cwd: PathBuf::from(field(cwd)?),
            status: parse(status)?,
            time: parse(time)?,
            command: field(command)?,
        })
    };
    build().ok_or(bad)
}

fn unknown(kind: &[u8]) -> DecodeError {
    DecodeError::UnknownKind(String::from_utf8_lossy(kind).into_owned())
}

/// Parses a number field; `None` for non-UTF-8 or non-numeric bytes.
fn parse<T: std::str::FromStr>(bytes: &[u8]) -> Option<T> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

/// A text field: UTF-8 with each `RS` read back as a newline.
fn field(bytes: &[u8]) -> Option<String> {
    let mapped: Vec<u8> = bytes
        .iter()
        .map(|&b| if b == RS { LF } else { b })
        .collect();
    String::from_utf8(mapped).ok()
}

fn join(parts: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.push(US);
        }
        debug_assert!(
            !part.bytes().any(|b| b == US || b == RS),
            "field holds a separator byte"
        );
        out.extend(part.bytes().map(|b| if b == LF { RS } else { b }));
    }
    out.push(LF);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const US: u8 = 0x1f;

    fn line(parts: &[&str]) -> Vec<u8> {
        parts.join("\x1f").into_bytes()
    }

    fn strip_lf(mut v: Vec<u8>) -> Vec<u8> {
        assert_eq!(v.pop(), Some(b'\n'));
        v
    }

    #[test]
    fn decodes_hello() {
        let r = decode_request(&line(&[
            "hello",
            "1",
            "zsh",
            "5.9",
            "zsh",
            "/h/.zsh_history",
            "autosuggestions",
        ]))
        .unwrap();
        assert_eq!(
            r,
            Request::Hello {
                protocol: 1,
                shell: "zsh".into(),
                shell_version: "5.9".into(),
                history_format: "zsh".into(),
                history_path: Some("/h/.zsh_history".into()),
                libraries: vec!["autosuggestions".into()],
            }
        );
    }

    #[test]
    fn empty_history_path_is_none() {
        let r = decode_request(&line(&[
            "hello",
            "1",
            "zsh",
            "5.9",
            "zsh",
            "",
            "autosuggestions",
        ]))
        .unwrap();
        match r {
            Request::Hello { history_path, .. } => assert_eq!(history_path, None),
            other => panic!("not a hello: {other:?}"),
        }
    }

    #[test]
    fn decodes_suggest_and_maps_rs_to_newline() {
        let r =
            decode_request(&line(&["suggest", "7", "async", "3", "/tmp", "for\x1edo"])).unwrap();
        assert_eq!(
            r,
            Request::Suggest {
                id: 7,
                mode: Mode::Async,
                cursor: 3,
                cwd: "/tmp".into(),
                buffer: "for\ndo".into()
            }
        );
    }

    #[test]
    fn decodes_sync_mode() {
        let r = decode_request(&line(&["suggest", "1", "sync", "0", "/", ""])).unwrap();
        match r {
            Request::Suggest { mode, .. } => assert_eq!(mode, Mode::Sync),
            other => panic!("not a suggest: {other:?}"),
        }
    }

    #[test]
    fn decodes_ran() {
        let r = decode_request(&line(&["ran", "/w", "1", "1794524352", "false"])).unwrap();
        assert_eq!(
            r,
            Request::Ran {
                cwd: "/w".into(),
                status: 1,
                time: 1794524352,
                command: "false".into()
            }
        );
    }

    #[test]
    fn decodes_bye() {
        assert_eq!(decode_request(b"bye").unwrap(), Request::Bye);
    }

    #[test]
    fn unknown_kind_is_reported() {
        assert_eq!(
            decode_request(b"menu"),
            Err(DecodeError::UnknownKind("menu".into()))
        );
    }

    #[test]
    fn bad_suggest_keeps_its_id() {
        assert_eq!(
            decode_request(&line(&["suggest", "9", "async", "x", "/tmp", "git"])),
            Err(DecodeError::Malformed { id: Some(9) })
        );
    }

    #[test]
    fn unknown_mode_is_malformed() {
        assert_eq!(
            decode_request(&line(&["suggest", "9", "later", "0", "/tmp", "git"])),
            Err(DecodeError::Malformed { id: Some(9) })
        );
    }

    #[test]
    fn wrong_field_count_is_malformed() {
        assert_eq!(
            decode_request(&line(&["ran", "/w"])),
            Err(DecodeError::Malformed { id: None })
        );
    }

    #[test]
    fn non_utf8_field_is_malformed() {
        let mut l = line(&["suggest", "5", "async", "0", "/tmp"]);
        l.push(US);
        l.push(0xff);
        assert_eq!(
            decode_request(&l),
            Err(DecodeError::Malformed { id: Some(5) })
        );
    }

    #[test]
    fn encodes_suggestion_with_newline_as_rs() {
        assert_eq!(
            encode_response(&Response::Suggestion {
                id: 4,
                text: Some("a\nb".into())
            }),
            b"suggestion\x1f4\x1fa\x1eb\n"
        );
    }

    #[test]
    fn none_encodes_as_empty_text() {
        assert_eq!(
            encode_response(&Response::Suggestion { id: 4, text: None }),
            b"suggestion\x1f4\x1f\n"
        );
    }

    #[test]
    fn text_containing_separators_encodes_as_none() {
        assert_eq!(
            encode_response(&Response::Suggestion {
                id: 4,
                text: Some("a\x1fb".into())
            }),
            b"suggestion\x1f4\x1f\n"
        );
        assert_eq!(
            encode_response(&Response::Suggestion {
                id: 4,
                text: Some("a\x1eb".into())
            }),
            b"suggestion\x1f4\x1f\n"
        );
    }

    #[test]
    fn encodes_ready() {
        assert_eq!(
            encode_response(&Response::Ready { protocol: 1 }),
            b"ready\x1f1\n"
        );
    }

    #[test]
    fn request_round_trips() {
        let reqs = [
            Request::Hello {
                protocol: PROTOCOL_VERSION,
                shell: "zsh".into(),
                shell_version: "5.9".into(),
                history_format: "zsh".into(),
                history_path: Some("/h/.zsh_history".into()),
                libraries: vec!["autosuggestions".into()],
            },
            Request::Hello {
                protocol: 1,
                shell: "zsh".into(),
                shell_version: "5.3".into(),
                history_format: "zsh".into(),
                history_path: None,
                libraries: vec![],
            },
            Request::Suggest {
                id: u64::MAX,
                mode: Mode::Sync,
                cursor: 4,
                cwd: "/tmp".into(),
                buffer: "a\nb".into(),
            },
            Request::Ran {
                cwd: "/w".into(),
                status: -1,
                time: 1794524352,
                command: "x\ny".into(),
            },
            Request::Bye,
        ];
        for r in reqs {
            assert_eq!(decode_request(&strip_lf(encode_request(&r))), Ok(r));
        }
    }

    #[test]
    fn response_round_trips() {
        let resps = [
            Response::Ready { protocol: 1 },
            Response::Suggestion {
                id: 3,
                text: Some(" status\nmore".into()),
            },
            Response::Suggestion { id: 3, text: None },
        ];
        for r in resps {
            assert_eq!(decode_response(&strip_lf(encode_response(&r))), Ok(r));
        }
    }

    #[test]
    fn empty_response_text_decodes_to_none() {
        assert_eq!(
            decode_response(b"suggestion\x1f8\x1f"),
            Ok(Response::Suggestion { id: 8, text: None })
        );
    }

    #[test]
    fn unknown_response_kind_is_reported() {
        assert_eq!(
            decode_response(b"items\x1f1"),
            Err(DecodeError::UnknownKind("items".into()))
        );
    }
}
