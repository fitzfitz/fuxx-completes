//! Parsing bash's history file (`$HISTFILE`) into commands. Read-only; see `docs/design.md` §5.1.

/// A `#` and a digit at the start of a line: bash's timestamp line (`HIST_TIMESTAMP_START` in
/// readline's `histfile.c`).
fn is_timestamp(line: &[u8]) -> bool {
    line.first() == Some(&b'#') && line.get(1).is_some_and(u8::is_ascii_digit)
}

/// Turns bash history-file bytes into commands, as bash itself reads its file back
/// (`read_history_range` in readline's `histfile.c`):
///
/// - a file whose first line is a timestamp (`#<digit>…`, written when `HISTTIMEFORMAT` is set)
///   has timestamps: the timestamp lines are dropped, and the lines between two timestamps are
///   one entry, joined with `\n` (a multi-line command saved with `lithist`);
/// - otherwise each non-blank line is an entry, and a timestamp-looking line is still dropped;
/// - a `\r` before the `\n` is trimmed; empty entries and entries that aren't UTF-8 are skipped.
///
/// With timestamps an entry ends only at the next timestamp, so [`feed`](Self::feed) holds the
/// newest one open; [`flush`](Self::flush) ends it at the end of what was written (bash writes
/// whole entries), and [`finish`](Self::finish) at the end of the file.
#[derive(Debug, Default, Clone)]
pub struct BashHistoryParser {
    /// Bytes of a line not yet ended by `\n`.
    partial: Vec<u8>,
    /// `Some(true)` once the first line was a timestamp, `Some(false)` for a plain file.
    timestamped: Option<bool>,
    /// The entry being built in a timestamped file.
    open: Option<Vec<u8>>,
}

impl BashHistoryParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Complete commands found in `bytes`, oldest first.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = bytes;
        while let Some(nl) = rest.iter().position(|&b| b == b'\n') {
            self.partial.extend_from_slice(&rest[..nl]);
            rest = &rest[nl + 1..];
            let mut line = std::mem::take(&mut self.partial);
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.line(line, &mut out);
        }
        self.partial.extend_from_slice(rest);
        out
    }

    /// Ends the open entry, unless an unfinished line could still belong to it.
    pub fn flush(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        if self.partial.is_empty() {
            if let Some(entry) = self.open.take() {
                push(entry, &mut out);
            }
        }
        out
    }

    /// The end of the file: an unfinished last line counts as a line, and the open entry ends.
    pub fn finish(mut self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.partial.is_empty() {
            let line = std::mem::take(&mut self.partial);
            self.line(line, &mut out);
        }
        out.extend(self.flush());
        out
    }

    fn line(&mut self, line: Vec<u8>, out: &mut Vec<String>) {
        let ts = is_timestamp(&line);
        let timestamped = *self.timestamped.get_or_insert(ts);
        if !timestamped {
            if !ts && !line.is_empty() {
                push(line, out);
            }
            return;
        }
        if ts {
            if let Some(entry) = self.open.take() {
                push(entry, out);
            }
            self.open = Some(Vec::new());
            // An entry that just started: the next line is its first.
            return;
        }
        match &mut self.open {
            Some(entry) if !entry.is_empty() => {
                entry.push(b'\n');
                entry.extend_from_slice(&line);
            }
            Some(entry) => *entry = line,
            // After a flush, a line with no timestamp before it starts an entry of its own.
            None => self.open = Some(line),
        }
    }
}

/// One entry to a command, or nothing when it is empty (blank lines trimmed off its end) or not
/// valid UTF-8.
fn push(mut entry: Vec<u8>, out: &mut Vec<String>) {
    while entry.last() == Some(&b'\n') {
        entry.pop();
    }
    if let Ok(command) = String::from_utf8(entry) {
        if !command.is_empty() {
            out.push(command);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(bytes: &[u8]) -> Vec<String> {
        let mut p = BashHistoryParser::new();
        let mut out = p.feed(bytes);
        out.extend(p.finish());
        out
    }

    const MULTI: &[u8] = b"#1\nfor i in 1 2\ndo echo $i\ndone\n#2\nls\n";

    #[test]
    fn plain_lines_are_entries() {
        assert_eq!(parse(b"ls\n\ngit status\n"), ["ls", "git status"]);
    }

    #[test]
    fn timestamps_are_dropped() {
        assert_eq!(
            parse(b"#1700000000\nls\n#1700000001\ngit st\n"),
            ["ls", "git st"]
        );
    }

    #[test]
    fn multiline_entries_join_between_timestamps() {
        assert_eq!(parse(MULTI), ["for i in 1 2\ndo echo $i\ndone", "ls"]);
    }

    #[test]
    fn hash_without_digit_is_a_command() {
        assert_eq!(parse(b"#foo\n"), ["#foo"]);
    }

    #[test]
    fn chunks_resume_mid_entry() {
        let mut p = BashHistoryParser::new();
        let mut out = Vec::new();
        for chunk in MULTI.chunks(3) {
            out.extend(p.feed(chunk));
        }
        out.extend(p.finish());
        assert_eq!(out, ["for i in 1 2\ndo echo $i\ndone", "ls"]);
    }

    #[test]
    fn crlf_is_trimmed() {
        assert_eq!(parse(b"ls\r\ngit status\r\n"), ["ls", "git status"]);
        assert_eq!(parse(b"#1\r\nls\r\n#2\r\npwd\r\n"), ["ls", "pwd"]);
    }

    #[test]
    fn a_timestamp_line_in_a_plain_file_is_dropped() {
        // bash drops a `#<digit>` line wherever it is (histfile.c); without a timestamp on the
        // first line, lines never join.
        assert_eq!(parse(b"ls\n#123\npwd\n"), ["ls", "pwd"]);
    }

    #[test]
    fn the_open_entry_waits_for_the_next_timestamp_but_flush_ends_it() {
        let mut p = BashHistoryParser::new();
        assert_eq!(p.feed(b"#1\nls\n"), Vec::<String>::new());
        assert_eq!(p.flush(), ["ls"]);
        assert_eq!(p.feed(b"#2\npwd\n"), Vec::<String>::new());
        assert_eq!(p.finish(), ["pwd"]);
    }

    #[test]
    fn flush_keeps_an_unfinished_line() {
        let mut p = BashHistoryParser::new();
        assert!(p.feed(b"ls\ngit st").contains(&"ls".to_string()));
        assert!(p.flush().is_empty());
        assert_eq!(p.feed(b"atus\n"), ["git status"]);
    }

    #[test]
    fn invalid_utf8_entry_is_skipped() {
        assert_eq!(parse(b"ok1\n\xff\xfe\nok2\n"), ["ok1", "ok2"]);
    }
}
