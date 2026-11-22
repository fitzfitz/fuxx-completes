//! The host channel (design §5.5): what a shell adapter tells a host terminal over the pty when
//! the host asked for it with `FUXX_COMPLETES_HOST=1`, and the key sequence the host types back to
//! accept a suggestion. Shared by any host that links this crate (fuxx, the tests' fake host).
//!
//! A message is `ESC ] 7701 ; fuxx-completes ; 1 ; <message> ; <fields…> BEL`. Numbers are plain
//! digits (with a `-` when negative); every other field is base64 (RFC 4648, padded; an empty
//! field stays empty), so no field can hold `;`, `BEL` or `ESC`. The accept is `ESC [ 7701 ~`,
//! then base64 of `<id> US <buffer> US <cursor>`, then `BEL`.

use std::path::PathBuf;

/// The OSC number (design §5.5; re-checked against current terminal docs in M2).
pub const OSC_NUMBER: &str = "7701";
/// The namespace: a host ignores any OSC 7701 whose first field isn't this.
pub const NAMESPACE: &str = "fuxx-completes";
/// The channel's protocol version.
pub const VERSION: &str = "1";
/// The CSI a host types to accept: `ESC [ 7701 ~`.
pub const ACCEPT_PREFIX: &[u8] = b"\x1b[7701~";

const US: char = '\u{1f}';

/// What a shell adapter sends a host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostMessage {
    /// Once, before anything else: the shell and its history file.
    Hello {
        shell: String,
        version: String,
        history_format: String,
        history_path: Option<PathBuf>,
        libraries: Vec<String>,
    },
    /// The line as it is now; `cursor` counts code points. The id is what an accept names.
    Buffer {
        id: u64,
        cursor: usize,
        cwd: PathBuf,
        buffer: String,
    },
    /// The line was run or abandoned.
    Clear,
    /// A command the shell saved (design §5.8), as `ran` to a core.
    Ran {
        cwd: PathBuf,
        status: i32,
        time: i64,
        command: String,
    },
    /// The command just run and its status, as `prev` to a core (`None`: unknown).
    Prev {
        command: Option<String>,
        status: Option<i32>,
    },
}

/// The OSC for `msg`, `ESC ] … BEL`.
pub fn encode_osc(msg: &HostMessage) -> Vec<u8> {
    let b64 = |s: &str| base64_encode(s.as_bytes());
    let path = |p: &std::path::Path| base64_encode(p.as_os_str().as_encoded_bytes());
    let (kind, fields): (&str, Vec<String>) = match msg {
        HostMessage::Hello {
            shell,
            version,
            history_format,
            history_path,
            libraries,
        } => (
            "hello",
            vec![
                b64(shell),
                b64(version),
                b64(history_format),
                history_path.as_deref().map(path).unwrap_or_default(),
                b64(&libraries.join(",")),
            ],
        ),
        HostMessage::Buffer {
            id,
            cursor,
            cwd,
            buffer,
        } => (
            "buffer",
            vec![id.to_string(), cursor.to_string(), path(cwd), b64(buffer)],
        ),
        HostMessage::Clear => ("clear", vec![]),
        HostMessage::Ran {
            cwd,
            status,
            time,
            command,
        } => (
            "ran",
            vec![
                path(cwd),
                status.to_string(),
                time.to_string(),
                b64(command),
            ],
        ),
        HostMessage::Prev { command, status } => (
            "prev",
            vec![
                command.as_deref().map(b64).unwrap_or_default(),
                status.map(|s| s.to_string()).unwrap_or_default(),
            ],
        ),
    };
    let mut body = format!("{OSC_NUMBER};{NAMESPACE};{VERSION};{kind}");
    for f in fields {
        body.push(';');
        body.push_str(&f);
    }
    let mut out = b"\x1b]".to_vec();
    out.extend_from_slice(body.as_bytes());
    out.push(0x07);
    out
}

/// One message from an OSC body (the bytes between `ESC ]` and the terminator), or `None` for
/// anything else: another OSC number, another namespace or version, an unknown message, or
/// fields that don't decode.
pub fn decode_osc(body: &[u8]) -> Option<HostMessage> {
    let body = std::str::from_utf8(body).ok()?;
    let mut parts = body.split(';');
    if parts.next()? != OSC_NUMBER || parts.next()? != NAMESPACE || parts.next()? != VERSION {
        return None;
    }
    let kind = parts.next()?;
    let f: Vec<&str> = parts.collect();
    let text = |s: &str| String::from_utf8(base64_decode(s)?).ok();
    let path = |s: &str| text(s).map(PathBuf::from);
    let opt_text = |s: &str| {
        if s.is_empty() {
            Some(None)
        } else {
            text(s).map(Some)
        }
    };
    match (kind, f.as_slice()) {
        ("hello", [shell, version, format, hist, libs]) => Some(HostMessage::Hello {
            shell: text(shell)?,
            version: text(version)?,
            history_format: text(format)?,
            history_path: opt_text(hist)?.map(PathBuf::from),
            libraries: text(libs)?
                .split(',')
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect(),
        }),
        ("buffer", [id, cursor, cwd, buffer]) => Some(HostMessage::Buffer {
            id: id.parse().ok()?,
            cursor: cursor.parse().ok()?,
            cwd: path(cwd)?,
            buffer: text(buffer)?,
        }),
        ("clear", []) => Some(HostMessage::Clear),
        ("ran", [cwd, status, time, command]) => Some(HostMessage::Ran {
            cwd: path(cwd)?,
            status: status.parse().ok()?,
            time: time.parse().ok()?,
            command: text(command)?,
        }),
        ("prev", [command, status]) => Some(HostMessage::Prev {
            command: opt_text(command)?,
            status: if status.is_empty() {
                None
            } else {
                Some(status.parse().ok()?)
            },
        }),
        _ => None,
    }
}

/// Every host message in `stream` (what a host reads from the pty), in order. Other OSCs, plain
/// text and a sequence cut off at the end are skipped. An OSC ends at `BEL` or `ESC \`.
pub fn scan(stream: &[u8]) -> Vec<HostMessage> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(start) = find(stream, b"\x1b]", i) {
        let body = start + 2;
        let mut end = None;
        let mut j = body;
        while j < stream.len() {
            match stream[j] {
                0x07 => {
                    end = Some((j, j + 1));
                    break;
                }
                0x1b if stream.get(j + 1) == Some(&b'\\') => {
                    end = Some((j, j + 2));
                    break;
                }
                0x1b => break, // another sequence starts: this one was cut off
                _ => j += 1,
            }
        }
        match end {
            Some((stop, next)) => {
                out.extend(decode_osc(&stream[body..stop]));
                i = next;
            }
            None => i = body,
        }
    }
    out
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// The key sequence a host types to put `buffer` (cursor in code points) into the line it last
/// saw as `id`: `ESC [ 7701 ~`, base64 of `<id> US <buffer> US <cursor>`, `BEL`.
pub fn encode_accept(id: u64, buffer: &str, cursor: usize) -> Vec<u8> {
    let mut out = ACCEPT_PREFIX.to_vec();
    out.extend_from_slice(
        base64_encode(format!("{id}{US}{buffer}{US}{cursor}").as_bytes()).as_bytes(),
    );
    out.push(0x07);
    out
}

/// The accept's payload (the base64 between `ESC [ 7701 ~` and `BEL`) as (id, buffer, cursor).
pub fn decode_accept(payload: &[u8]) -> Option<(u64, String, usize)> {
    let text = String::from_utf8(base64_decode(std::str::from_utf8(payload).ok()?)?).ok()?;
    let (id, rest) = text.split_once(US)?;
    let (buffer, cursor) = rest.rsplit_once(US)?;
    Some((id.parse().ok()?, buffer.to_string(), cursor.parse().ok()?))
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 (RFC 4648, standard alphabet, padded).
pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes padded base64; `None` for a bad length, a bad character or bad padding.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let s = text.as_bytes();
    if !s.len().is_multiple_of(4) {
        return None;
    }
    let val = |c: u8| B64.iter().position(|&b| b == c).map(|p| p as u32);
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    for (i, q) in s.chunks(4).enumerate() {
        let last = i == s.len() / 4 - 1;
        let pad = q.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && !last) {
            return None;
        }
        let mut n = 0u32;
        for &c in &q[..4 - pad] {
            n = n << 6 | val(c)?;
        }
        n <<= 6 * pad as u32;
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - pad]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_message() -> Vec<HostMessage> {
        vec![
            HostMessage::Hello {
                shell: "zsh".into(),
                version: "5.9".into(),
                history_format: "zsh".into(),
                history_path: Some("/h/.zsh_history".into()),
                libraries: vec!["autosuggestions".into()],
            },
            HostMessage::Hello {
                shell: "bash".into(),
                version: "5.3.0(1)-release".into(),
                history_format: "bash".into(),
                history_path: None,
                libraries: vec![],
            },
            HostMessage::Buffer {
                id: 7,
                cursor: 5,
                cwd: "/w".into(),
                buffer: "git s".into(),
            },
            HostMessage::Clear,
            HostMessage::Ran {
                cwd: "/w".into(),
                status: -1,
                time: 1_700_000_000,
                command: "false".into(),
            },
            HostMessage::Prev {
                command: Some("false".into()),
                status: Some(1),
            },
            HostMessage::Prev {
                command: None,
                status: None,
            },
        ]
    }

    fn body(osc: &[u8]) -> &[u8] {
        assert!(
            osc.starts_with(b"\x1b]") && osc.ends_with(b"\x07"),
            "{osc:?}"
        );
        &osc[2..osc.len() - 1]
    }

    #[test]
    fn base64_matches_rfc4648() {
        for (plain, enc) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64_encode(plain.as_bytes()), enc);
            assert_eq!(base64_decode(enc).unwrap(), plain.as_bytes());
        }
        for bad in ["Zg=", "Z===", "Zg==Zg==", "Zm9*", "Zm9v\n"] {
            assert_eq!(base64_decode(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn osc_round_trips_every_message() {
        for m in every_message() {
            let osc = encode_osc(&m);
            assert_eq!(decode_osc(body(&osc)), Some(m.clone()), "{osc:?}");
        }
    }

    #[test]
    fn hostile_text_round_trips() {
        let m = HostMessage::Buffer {
            id: 3,
            cursor: 9,
            cwd: "/a;b/\u{7}\u{1b}".into(),
            buffer: "echo \"a;b\" '\u{7}' é 🙂".into(),
        };
        let osc = encode_osc(&m);
        let b = body(&osc);
        assert!(
            b.iter()
                .all(|c| c.is_ascii_alphanumeric() || b"+/=;-".contains(c)),
            "{:?}",
            String::from_utf8_lossy(b)
        );
        assert_eq!(decode_osc(b), Some(m));
    }

    #[test]
    fn other_namespaces_and_versions_are_ignored() {
        assert_eq!(decode_osc(b"7701;other;1;clear"), None);
        assert_eq!(decode_osc(b"7701;fuxx-completes;2;clear"), None);
        assert_eq!(decode_osc(b"7702;fuxx-completes;1;clear"), None);
        assert_eq!(decode_osc(b"7701;fuxx-completes;1;nope"), None);
        assert_eq!(decode_osc(b"7701;fuxx-completes;1;buffer;x;1;Lw==;"), None);
        assert_eq!(
            decode_osc(b"7701;fuxx-completes;1;clear"),
            Some(HostMessage::Clear)
        );
    }

    #[test]
    fn scan_finds_messages_among_text() {
        let mut s = b"plain text \x1b]0;title\x07 more ".to_vec();
        s.extend(encode_osc(&HostMessage::Clear));
        s.extend(b"\x1b]133;A\x1b\\");
        // ESC \ ends ours too.
        let mut st = encode_osc(&HostMessage::Prev {
            command: None,
            status: None,
        });
        st.pop();
        st.extend(b"\x1b\\");
        s.extend(st);
        s.extend(b"READY> ");
        // A cut-off sequence at the end.
        let cut = encode_osc(&HostMessage::Clear);
        s.extend(&cut[..cut.len() - 3]);
        assert_eq!(
            scan(&s),
            vec![
                HostMessage::Clear,
                HostMessage::Prev {
                    command: None,
                    status: None
                }
            ]
        );
    }

    #[test]
    fn accept_round_trips() {
        let a = encode_accept(42, "git status é", 12);
        assert!(a.starts_with(ACCEPT_PREFIX) && a.ends_with(b"\x07"));
        let payload = &a[ACCEPT_PREFIX.len()..a.len() - 1];
        assert_eq!(
            decode_accept(payload),
            Some((42, "git status é".to_string(), 12))
        );
        assert_eq!(decode_accept(b"!!!!"), None);
        assert_eq!(
            decode_accept(base64_encode(b"1\x1fno cursor").as_bytes()),
            None
        );
    }
}
