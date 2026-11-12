//! Parsing zsh's history file (`$HISTFILE`) into commands. Read-only; see `docs/design.md`.

/// Undo zsh's "metafication": a `0x83` byte marks that the next byte was stored XOR `0x20`.
pub fn unmetafy(bytes: &[u8]) -> Vec<u8> {
    const META: u8 = 0x83;
    let mut out = Vec::with_capacity(bytes.len());
    let mut iter = bytes.iter().copied();
    while let Some(b) = iter.next() {
        if b == META {
            match iter.next() {
                Some(next) => out.push(next ^ 0x20),
                None => out.push(b), // dangling marker: keep it; the UTF-8 check drops the entry
            }
        } else {
            out.push(b);
        }
    }
    out
}

/// Strip a leading EXTENDED_HISTORY header, `: <digits>:<digits>;`. Anything else is returned
/// unchanged, so a command that merely starts with `: ` is kept.
fn strip_header(entry: &[u8]) -> &[u8] {
    let Some(rest) = entry.strip_prefix(b": ") else {
        return entry;
    };
    let digits = |s: &[u8]| s.iter().take_while(|b| b.is_ascii_digit()).count();
    let start = digits(rest);
    if start == 0 || rest.get(start) != Some(&b':') {
        return entry;
    }
    let rest = &rest[start + 1..];
    let elapsed = digits(rest);
    if elapsed == 0 || rest.get(elapsed) != Some(&b';') {
        return entry;
    }
    &rest[elapsed + 1..]
}

/// One raw entry (continuation lines already joined) to a command, or `None` if it should
/// never be suggested: empty, or not valid UTF-8.
fn entry_to_command(raw: &[u8]) -> Option<String> {
    let bytes = unmetafy(raw);
    let command = String::from_utf8(strip_header(&bytes).to_vec()).ok()?;
    (!command.is_empty()).then_some(command)
}

/// Turns history-file bytes into commands, keeping the bytes of an unfinished entry between
/// calls to [`feed`](Self::feed).
#[derive(Debug, Default, Clone)]
pub struct ZshHistoryParser {
    pending: Vec<u8>,
}

impl ZshHistoryParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Complete commands found in `bytes`, oldest first.
    ///
    /// An entry is complete at a LF that isn't preceded by `\`; a `\`+LF inside an entry is
    /// zsh's line continuation and becomes a plain "\n" in the command (zsh's hist.c).
    /// Empty entries and entries that aren't valid UTF-8 are skipped.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(bytes);
        let mut commands = Vec::new();
        let mut entry = Vec::new(); // current entry with continuations joined
        let mut consumed = 0; // bytes of `pending` belonging to finished entries
        for (i, &b) in self.pending.iter().enumerate() {
            if b != b'\n' {
                entry.push(b);
            } else if entry.last() == Some(&b'\\') {
                // Continuation: drop the backslash, keep the line break.
                entry.pop();
                entry.push(b'\n');
            } else {
                commands.extend(entry_to_command(&entry));
                entry.clear();
                consumed = i + 1;
            }
        }
        self.pending.drain(..consumed);
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_all(bytes: &[u8]) -> Vec<String> {
        ZshHistoryParser::new().feed(bytes)
    }

    #[test]
    fn plain_lines_are_commands() {
        assert_eq!(feed_all(b"ls\ngit status\n"), ["ls", "git status"]);
    }

    #[test]
    fn extended_header_is_stripped() {
        assert_eq!(feed_all(b": 1794524352:0;git push\n"), ["git push"]);
    }

    #[test]
    fn header_lookalike_without_digits_is_kept() {
        assert_eq!(feed_all(b": x;y\n"), [": x;y"]);
    }

    #[test]
    fn backslash_newline_joins_multiline_command() {
        assert_eq!(
            feed_all(b"for i in 1 2\\\ndo echo $i\\\ndone\n"),
            ["for i in 1 2\ndo echo $i\ndone"]
        );
    }

    #[test]
    fn metafied_bytes_are_decoded() {
        // 日 is e6 97 a5; zsh stores the 0x97 as 0x83 0xb7.
        assert_eq!(feed_all(b"echo \xe6\x83\xb7\xa5\n"), ["echo 日"]);
    }

    #[test]
    fn unterminated_last_line_waits_for_more() {
        let mut p = ZshHistoryParser::new();
        assert_eq!(p.feed(b"ls\ngit st"), ["ls"]);
        assert_eq!(p.feed(b"atus\n"), ["git status"]);
    }

    #[test]
    fn continuation_split_across_feeds() {
        let mut p = ZshHistoryParser::new();
        assert!(p.feed(b"a\\").is_empty());
        assert_eq!(p.feed(b"\nb\n"), ["a\nb"]);
    }

    #[test]
    fn invalid_utf8_entry_is_skipped_others_kept() {
        assert_eq!(feed_all(b"ok1\n\xff\xfe\nok2\n"), ["ok1", "ok2"]);
    }

    #[test]
    fn empty_lines_are_skipped() {
        assert_eq!(feed_all(b"\n\nls\n"), ["ls"]);
    }
}
