//! Keeps a [`History`] in step with the history file on disk. Read-only: the file is only ever
//! opened for reading.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::thread::JoinHandle;

use crate::history::{History, HistoryFormat};
use crate::zsh_history::ZshHistoryParser;

/// What we know about the file we last read. `len` is the number of bytes we consumed (not
/// the file's length at some other moment), so "longer than `len`" means "has unread bytes".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    dev: u64,
    ino: u64,
    len: u64,
    mtime: (i64, i64),
}

impl Stamp {
    fn of(meta: &std::fs::Metadata, len: u64) -> Self {
        Stamp {
            dev: meta.dev(),
            ino: meta.ino(),
            len,
            mtime: (meta.mtime(), meta.mtime_nsec()),
        }
    }
}

/// The result of a background full read: everything needed to carry on reading incrementally
/// from exactly where the full read stopped.
struct FullRead {
    entries: Vec<String>,
    parser: ZshHistoryParser,
    /// `None` when the file could not be opened (missing = no entries).
    stamp: Option<Stamp>,
}

/// Test hook: told when the read is finished, then holds the result back until released.
#[cfg(test)]
type Hold = (std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>);
#[cfg(not(test))]
type Hold = ();

#[derive(Debug)]
pub struct HistoryFile {
    path: PathBuf,
    // Only zsh exists in M0; kept so a second format has one place to plug in.
    #[allow(dead_code)]
    format: HistoryFormat,
    /// The file as of the last read that was applied; `None` = no file (or nothing applied).
    seen: Option<Stamp>,
    parser: ZshHistoryParser,
    reload: Option<JoinHandle<FullRead>>,
}

impl HistoryFile {
    /// Starts a background full read at once; never blocks.
    pub fn new(path: PathBuf, format: HistoryFormat) -> Self {
        Self::start(path, format, None)
    }

    #[cfg(test)]
    fn new_held(
        path: PathBuf,
        format: HistoryFormat,
    ) -> (
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::Sender<()>,
        Self,
    ) {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let f = Self::start(path, format, Some((done_tx, release_rx)));
        (done_rx, release_tx, f)
    }

    fn start(path: PathBuf, format: HistoryFormat, hold: Option<Hold>) -> Self {
        let mut f = HistoryFile {
            path,
            format,
            seen: None,
            parser: ZshHistoryParser::new(),
            reload: None,
        };
        f.start_reload(hold);
        f
    }

    pub fn is_reloading(&self) -> bool {
        self.reload.is_some()
    }

    /// Never blocks on a full read. Applies a finished background read; then stats the file:
    /// same device+inode and longer, read only the new bytes inline; any other change (new
    /// inode, shorter, same length but new mtime), start a background full read unless one is
    /// running. A missing file is "no entries".
    pub fn refresh(&mut self, history: &mut History) {
        if let Some(handle) = &self.reload {
            if !handle.is_finished() {
                // A growth now would be appended to a History the full read is about to
                // replace; leave it, the incremental read after the swap picks it up.
                return;
            }
            let handle = self.reload.take().expect("checked above");
            if let Ok(done) = handle.join() {
                history.replace_file_entries(done.entries);
                self.parser = done.parser;
                self.seen = done.stamp;
            } // a panicked reader leaves the old index; the stat below retries if needed
        }

        let meta = match std::fs::metadata(&self.path) {
            Ok(m) => m,
            Err(_) => {
                if self.seen.take().is_some() {
                    self.parser = ZshHistoryParser::new();
                    history.replace_file_entries(Vec::new());
                }
                return;
            }
        };
        match self.seen {
            Some(seen) if seen.dev == meta.dev() && seen.ino == meta.ino() => {
                if meta.len() > seen.len {
                    self.read_new_bytes(history, seen);
                } else if meta.len() < seen.len || (meta.mtime(), meta.mtime_nsec()) != seen.mtime {
                    self.start_reload(None);
                }
            }
            // A different file (or nothing applied yet), anything but a growth: full read.
            _ => self.start_reload(None),
        }
    }

    /// Reads the bytes after `seen.len` inline and feeds them to the parser, which keeps
    /// the bytes of an unfinished entry for the next call.
    fn read_new_bytes(&mut self, history: &mut History, seen: Stamp) {
        let Ok(mut file) = File::open(&self.path) else {
            return;
        };
        let Ok(meta) = file.metadata() else { return };
        // Open file, not path: make sure it is still the file we were reading.
        if meta.dev() != seen.dev || meta.ino() != seen.ino || meta.len() < seen.len {
            return; // next refresh sees the change through the path and reloads
        }
        if file.seek(SeekFrom::Start(seen.len)).is_err() {
            return;
        }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return;
        }
        let entries = self.parser.feed(&bytes);
        self.seen = Some(Stamp::of(&meta, seen.len + bytes.len() as u64));
        history.append_file_entries(entries);
    }

    fn start_reload(&mut self, hold: Option<Hold>) {
        if self.reload.is_some() {
            return;
        }
        let path = self.path.clone();
        self.reload = Some(std::thread::spawn(move || {
            let done = read_everything(&path);
            #[cfg(test)]
            if let Some((done_tx, release_rx)) = hold {
                let _ = done_tx.send(());
                let _ = release_rx.recv();
            }
            #[cfg(not(test))]
            let _ = hold;
            done
        }));
    }
}

/// Reads the whole file in chunks (so a huge file isn't held twice in memory).
fn read_everything(path: &PathBuf) -> FullRead {
    let mut parser = ZshHistoryParser::new();
    let mut entries = Vec::new();
    let Ok(mut file) = File::open(path) else {
        return FullRead {
            entries,
            parser,
            stamp: None,
        };
    };
    let mut total = 0u64;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match file.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                total += n as u64;
                entries.extend(parser.feed(&buf[..n]));
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    // Stat the open file after reading: same inode as what we read, and `len` is what we
    // consumed, so growth after (or during) the read still counts as unread bytes.
    let stamp = file.metadata().ok().map(|m| Stamp::of(&m, total));
    FullRead {
        entries,
        parser,
        stamp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::time::{Duration, Instant};

    fn settle(f: &mut HistoryFile, h: &mut History) {
        let start = Instant::now();
        loop {
            f.refresh(h);
            if !f.is_reloading() {
                return;
            }
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "reload did not finish"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn append(path: &std::path::Path, text: &str) {
        let mut file = OpenOptions::new().append(true).open(path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    fn entries(h: &History) -> Vec<&str> {
        h.file_entries().iter().map(String::as_str).collect()
    }

    #[test]
    fn first_read_happens_in_background() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        fs::write(&path, "ls\n").unwrap();
        let mut h = History::new();
        let mut f = HistoryFile::new(path, HistoryFormat::Zsh);
        settle(&mut f, &mut h);
        assert_eq!(h.latest_with_prefix("l"), Some("ls"));
    }

    #[test]
    fn missing_file_gives_no_entries_and_no_panic() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::new();
        let mut f = HistoryFile::new(dir.path().join("nope"), HistoryFormat::Zsh);
        settle(&mut f, &mut h);
        assert_eq!(h.latest_with_prefix(""), None);
    }

    #[test]
    fn file_deleted_later_clears_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        fs::write(&path, "ls\n").unwrap();
        let mut h = History::new();
        let mut f = HistoryFile::new(path.clone(), HistoryFormat::Zsh);
        settle(&mut f, &mut h);
        fs::remove_file(&path).unwrap();
        settle(&mut f, &mut h);
        assert_eq!(h.latest_with_prefix(""), None);
    }

    #[test]
    fn appended_lines_are_seen_without_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        fs::write(&path, "ls\n").unwrap();
        let mut h = History::new();
        let mut f = HistoryFile::new(path.clone(), HistoryFormat::Zsh);
        settle(&mut f, &mut h);
        append(&path, "git status\n");
        f.refresh(&mut h); // one call, no settle
        assert_eq!(h.latest_with_prefix("git"), Some("git status"));
        assert!(!f.is_reloading());
    }

    #[test]
    fn partial_line_is_completed_by_next_append() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        fs::write(&path, "ls\n").unwrap();
        let mut h = History::new();
        let mut f = HistoryFile::new(path.clone(), HistoryFormat::Zsh);
        settle(&mut f, &mut h);
        append(&path, "git st");
        f.refresh(&mut h);
        assert_eq!(h.latest_with_prefix("git"), None);
        append(&path, "atus\n");
        f.refresh(&mut h);
        assert_eq!(h.latest_with_prefix("git"), Some("git status"));
    }

    #[test]
    fn rewritten_file_is_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        fs::write(&path, "secret\nls\n").unwrap();
        let mut h = History::new();
        let mut f = HistoryFile::new(path.clone(), HistoryFormat::Zsh);
        settle(&mut f, &mut h);
        assert_eq!(h.latest_with_prefix("sec"), Some("secret"));
        let sibling = dir.path().join("hist.new");
        fs::write(&sibling, "ls\n").unwrap();
        fs::rename(&sibling, &path).unwrap(); // new inode
        settle(&mut f, &mut h);
        assert_eq!(h.latest_with_prefix("sec"), None);
    }

    #[test]
    fn truncated_file_is_reloaded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        fs::write(&path, "secret\nls\n").unwrap();
        let mut h = History::new();
        let mut f = HistoryFile::new(path.clone(), HistoryFormat::Zsh);
        settle(&mut f, &mut h);
        File::create(&path).unwrap().write_all(b"ls\n").unwrap(); // same inode, shorter
        settle(&mut f, &mut h);
        assert_eq!(h.latest_with_prefix("sec"), None);
        assert_eq!(h.latest_with_prefix("l"), Some("ls"));
    }

    #[test]
    fn large_history_loads_off_the_request_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        let mut text = String::new();
        for i in 0..100_000 {
            text.push_str(&format!("cmd {i}\n"));
        }
        fs::write(&path, text).unwrap();
        let mut h = History::new();
        // The test hook holds the finished read back until we release it, so
        // "still reloading" is certain, not a race against a fast disk.
        let (done_rx, release_tx, mut f) = HistoryFile::new_held(path, HistoryFormat::Zsh);
        done_rx.recv().unwrap();
        let start = Instant::now();
        f.refresh(&mut h);
        assert!(
            start.elapsed() < Duration::from_millis(5),
            "refresh blocked"
        );
        assert!(f.is_reloading());
        assert_eq!(h.latest_with_prefix("cmd 99999"), None);
        release_tx.send(()).unwrap();
        settle(&mut f, &mut h);
        assert_eq!(h.latest_with_prefix("cmd 99999"), Some("cmd 99999"));
    }

    #[test]
    fn growth_during_a_full_read_is_neither_lost_nor_duplicated() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        fs::write(&path, "a\nb\nc").unwrap(); // "c" is an unfinished entry
        let mut h = History::new();
        let (done_rx, release_tx, mut f) = HistoryFile::new_held(path.clone(), HistoryFormat::Zsh);
        done_rx.recv().unwrap(); // the full read has seen "a\nb\nc" and is held
        append(&path, "d\ne\n");
        f.refresh(&mut h); // still reloading: must not append to `h`
        assert!(entries(&h).is_empty());
        release_tx.send(()).unwrap();
        settle(&mut f, &mut h);
        assert_eq!(entries(&h), ["a", "b", "cd", "e"]);
    }

    #[test]
    fn nothing_is_written_to_the_history_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hist");
        fs::write(&path, "ls\n").unwrap();
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        let mut h = History::new();
        let mut f = HistoryFile::new(path.clone(), HistoryFormat::Zsh);
        settle(&mut f, &mut h);
        f.refresh(&mut h);
        assert_eq!(fs::read(&path).unwrap(), b"ls\n");
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    }
}
