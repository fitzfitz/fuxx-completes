//! One context record per line (`docs/design.md` §5.7):
//! `1 <32 hex key> <status> <time> <folder>\n`, the folder %-escaped.

//!
//! In the folder, bytes `0x00`–`0x1F`, `0x7F` and `%` are written `%XX`, so a line never holds
//! a raw LF and a torn write damages only its own line. A line that doesn't decode is skipped.

use crate::context::Key;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

pub const RECORD_VERSION: u8 = 1;

/// One command run: its key, the folder it started in, its exit status and the time (Unix
/// seconds). Never the command text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub key: Key,
    pub folder: PathBuf,
    pub status: i32,
    pub time: i64,
}

/// The record as one line, LF included.
pub fn encode(r: &Record) -> Vec<u8> {
    let mut out = format!(
        "{RECORD_VERSION} {} {} {} ",
        r.key.to_hex(),
        r.status,
        r.time
    )
    .into_bytes();
    for &b in r.folder.as_os_str().as_bytes() {
        if b < 0x20 || b == 0x7f || b == b'%' {
            out.extend_from_slice(format!("%{b:02X}").as_bytes());
        } else {
            out.push(b);
        }
    }
    out.push(b'\n');
    out
}

/// A line (without its LF) back into a record; `None` for anything that isn't a whole
/// version-1 line with an absolute folder.
pub fn decode(line: &[u8]) -> Option<Record> {
    let mut parts = line.splitn(5, |&b| b == b' ');
    let version = parts.next()?;
    if version != RECORD_VERSION.to_string().as_bytes() {
        return None;
    }
    let key = Key::from_hex(std::str::from_utf8(parts.next()?).ok()?)?;
    let status = std::str::from_utf8(parts.next()?).ok()?.parse().ok()?;
    let time = std::str::from_utf8(parts.next()?).ok()?.parse().ok()?;
    let folder = unescape(parts.next()?)?;
    if folder.first() != Some(&b'/') {
        return None;
    }
    Some(Record {
        key,
        folder: PathBuf::from(OsStr::from_bytes(&folder)),
        status,
        time,
    })
}

fn unescape(field: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(field.len());
    let mut i = 0;
    while i < field.len() {
        let b = field[i];
        if b == b'%' {
            let hex = std::str::from_utf8(field.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else if b < 0x20 || b == 0x7f {
            return None;
        } else {
            out.push(b);
            i += 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Key;
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;

    fn rec(folder: PathBuf) -> Record {
        Record {
            key: Key(42),
            folder,
            status: -3,
            time: 1_795_000_000,
        }
    }

    #[test]
    fn record_round_trips_odd_folders() {
        let folders = [
            PathBuf::from("/a b"),
            PathBuf::from("/50%"),
            PathBuf::from("/x\ny"),
            PathBuf::from("/t\tz"),
            PathBuf::from("/r\r\u{7f}"),
            PathBuf::from(OsStr::from_bytes(b"/\xff\xfe")),
            PathBuf::from("/日本"),
        ];
        for f in folders {
            let line = encode(&rec(f.clone()));
            assert_eq!(line.iter().filter(|&&b| b == b'\n').count(), 1, "{f:?}");
            assert_eq!(line.last(), Some(&b'\n'));
            let back = decode(&line[..line.len() - 1]).unwrap();
            assert_eq!(back, rec(f));
        }
    }

    #[test]
    fn encoded_line_has_the_documented_shape() {
        let line = encode(&rec(PathBuf::from("/p q")));
        assert_eq!(
            line,
            b"1 0000000000000000000000000000002a -3 1795000000 /p q\n".to_vec()
        );
    }

    #[test]
    fn decode_skips_bad_lines() {
        let good = "1 0000000000000000000000000000002a 0 5 /p";
        assert!(decode(good.as_bytes()).is_some());
        for bad in [
            &b""[..],
            b"2 0000000000000000000000000000002a 0 5 /p",
            b"1 000000000000000000000000000002a 0 5 /p",
            b"1 0000000000000000000000000000002a x 5 /p",
            b"1 0000000000000000000000000000002a 0 y /p",
            b"1 0000000000000000000000000000002a 0 5",
            b"1 0000000000000000000000000000002a 0 5 ",
            b"1 0000000000000000000000000000002a 0 5 /p%zz",
            b"1 0000000000000000000000000000002a 0 5 rel",
            b"\xff\xff",
        ] {
            assert_eq!(decode(bad), None, "{:?}", String::from_utf8_lossy(bad));
        }
    }
}
