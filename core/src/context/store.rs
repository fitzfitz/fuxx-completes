//! Writing and reading the context files under the data folder (`docs/design.md` §5.7).

//!
//! Each core appends to `context/<id>` for its own history file (`own_file_id`), and reads every
//! file in `context/`. A `context/<id>.lock` beside each file is taken shared by appends and
//! exclusive by a compaction, always with `try_lock`: nobody ever waits on it.

use crate::context::index::ContextIndex;
use crate::context::record::{decode, encode, Record};
use crate::context::secret::{make_private_dir, Secret, SECRET_FILE};
use crate::context::{DataDir, Key};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions, TryLockError};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

pub const CONTEXT_DIR: &str = "context";

#[derive(Debug)]
pub enum AppendError {
    /// A compaction holds the file's lock; try again later.
    Busy,
    Io(io::Error),
}

impl From<io::Error> for AppendError {
    fn from(e: io::Error) -> Self {
        AppendError::Io(e)
    }
}

/// The name of the file this core writes: the keyed hash of its history file's path, in hex.
pub fn own_file_id(secret: &Secret, history_path: &Path) -> String {
    secret
        .key_of_bytes(history_path.as_os_str().as_bytes())
        .to_hex()
}

/// Appends `records` to `<root>/context/<id>` in one write, holding the file's lock shared.
/// Makes the folders (`0700`) and the file (`0600`) as needed.
pub fn append(root: &Path, id: &str, records: &[Record]) -> Result<(), AppendError> {
    let dir = root.join(CONTEXT_DIR);
    make_private_dir(root)?;
    make_private_dir(&dir)?;
    let lock = open_lock(&dir, id)?;
    match lock.try_lock_shared() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Err(AppendError::Busy),
        Err(TryLockError::Error(e)) => return Err(e.into()),
    }
    let bytes: Vec<u8> = records.iter().flat_map(encode).collect();
    let mut f = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(dir.join(id))?;
    f.write_all(&bytes)?;
    Ok(())
}

pub(crate) fn open_lock(dir: &Path, id: &str) -> io::Result<fs::File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(dir.join(format!("{id}.lock")))
}

/// Deletes `<root>/context/` (everything in it) and `<root>/key`; then, when asked, `root`
/// itself if nothing else is left in it. True when something was removed.
pub fn forget(root: &Path, remove_root_if_empty: bool) -> io::Result<bool> {
    let mut removed = false;
    match fs::remove_dir_all(root.join(CONTEXT_DIR)) {
        Ok(()) => removed = true,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    match fs::remove_file(root.join(SECRET_FILE)) {
        Ok(()) => removed = true,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    if remove_root_if_empty {
        if let Ok(mut entries) = fs::read_dir(root) {
            if entries.next().is_none() {
                fs::remove_dir(root)?;
            }
        }
    }
    Ok(removed)
}

/// A file over this size is compacted (design §5.7: "a few MB").
pub const CAP: u64 = 4 * 1024 * 1024;
/// A compaction that trims stops here, so the next one isn't one append away.
pub const TRIM_TO: u64 = 3 * 1024 * 1024;
/// Records kept in memory while their file can't be written.
const MAX_PENDING: usize = 1000;

/// What we know about a context file we read: which file it was and how far we read it.
#[derive(Debug, Clone)]
struct FileState {
    dev: u64,
    ino: u64,
    consumed: u64,
    /// Bytes after the last LF: a line still being written.
    tail: Vec<u8>,
}

impl FileState {
    /// Feeds newly read bytes; every completed line that decodes goes to `index`.
    fn feed(&mut self, bytes: &[u8], index: &mut ContextIndex) {
        self.consumed += bytes.len() as u64;
        self.tail.extend_from_slice(bytes);
        if let Some(end) = self.tail.iter().rposition(|&b| b == b'\n') {
            for line in self.tail[..end].split(|&b| b == b'\n') {
                if let Some(r) = decode(line) {
                    index.add(&r);
                }
            }
            self.tail.drain(..=end);
        }
    }
}

/// A full read of every context file (done on a worker thread after a file was replaced).
struct Loaded {
    index: ContextIndex,
    files: HashMap<String, FileState>,
}

/// The names in `context/` that are context files (32 hex digits; not locks or temporaries).
fn context_ids(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| Key::from_hex(n).is_some())
        .collect()
}

/// Reads `path` from `state.consumed` on (a fresh state reads it all) into `index`.
fn read_into(path: &Path, state: &mut FileState, index: &mut ContextIndex) -> io::Result<()> {
    let mut f = fs::File::open(path)?;
    let meta = f.metadata()?;
    if (meta.dev(), meta.ino()) != (state.dev, state.ino) {
        *state = FileState {
            dev: meta.dev(),
            ino: meta.ino(),
            consumed: 0,
            tail: Vec::new(),
        };
    }
    f.seek(SeekFrom::Start(state.consumed))?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)?;
    state.feed(&bytes, index);
    Ok(())
}

fn load_all(dir: &Path) -> Loaded {
    let mut index = ContextIndex::default();
    let mut files = HashMap::new();
    for id in context_ids(dir) {
        let mut state = FileState {
            dev: 0,
            ino: 0,
            consumed: 0,
            tail: Vec::new(),
        };
        if read_into(&dir.join(&id), &mut state, &mut index).is_ok() {
            files.insert(id, state);
        }
    }
    Loaded { index, files }
}

/// The context files of one data folder, as one core sees them: it reads all of them and
/// writes only its own (the one for its history file). Never fails: a folder it can't read or
/// write just means no context.
pub struct ContextStore {
    dir: DataDir,
    history_path: Option<PathBuf>,
    secret: Option<Secret>,
    /// The key file's (device, inode) when the secret was loaded: `forget` changes or removes it.
    key_stamp: Option<(u64, u64)>,
    own_id: Option<String>,
    files: HashMap<String, FileState>,
    /// What the files say.
    base: ContextIndex,
    /// `base` plus the pending records, while there are any.
    view: Option<ContextIndex>,
    pending: Vec<Record>,
    rebuild: Option<JoinHandle<Loaded>>,
    compaction: Option<JoinHandle<()>>,
}

impl ContextStore {
    /// Loads the secret (if there is one) and reads every context file.
    pub fn open(dir: DataDir, history_path: Option<PathBuf>) -> ContextStore {
        let mut store = ContextStore {
            dir,
            history_path,
            secret: None,
            key_stamp: None,
            own_id: None,
            files: HashMap::new(),
            base: ContextIndex::default(),
            view: None,
            pending: Vec::new(),
            rebuild: None,
            compaction: None,
        };
        store.check_secret();
        if store.secret.is_some() {
            let loaded = load_all(&store.context_dir());
            store.base = loaded.index;
            store.files = loaded.files;
        }
        store
    }

    pub fn dir(&self) -> &DataDir {
        &self.dir
    }

    pub fn secret(&self) -> Option<&Secret> {
        self.secret.as_ref()
    }

    /// The tallies of every record read so far, pending ones included.
    pub fn index(&self) -> &ContextIndex {
        self.view.as_ref().unwrap_or(&self.base)
    }

    fn context_dir(&self) -> PathBuf {
        self.dir.root.join(CONTEXT_DIR)
    }

    /// Before each answer: notices `forget` (the key file gone or replaced: everything is
    /// dropped) or a secret made by another core, writes pending records, reads what the files
    /// gained, and starts (or swaps in) a full re-read when a file was replaced or removed.
    pub fn refresh(&mut self) {
        self.check_secret();
        if self.secret.is_none() {
            return;
        }
        self.flush_pending();
        if let Some(handle) = self.rebuild.take() {
            if handle.is_finished() {
                if let Ok(loaded) = handle.join() {
                    self.base = loaded.index;
                    self.files = loaded.files;
                }
            } else {
                self.rebuild = Some(handle);
                return;
            }
        }
        self.read_changes();
        self.update_view();
    }

    /// Records one command run; a no-op without a history file. Makes the secret on first use.
    /// A record that can't be written yet stays pending (and counts) until a later refresh.
    pub fn record(&mut self, command: &str, folder: &Path, status: i32, time: i64) {
        if self.history_path.is_none() {
            return;
        }
        if self.secret.is_none() {
            if Secret::load_or_create(&self.dir.root).is_err() {
                return;
            }
            self.check_secret();
        }
        let Some(secret) = &self.secret else {
            return;
        };
        self.pending.push(Record {
            key: secret.key_of(command),
            folder: folder.to_path_buf(),
            status,
            time,
        });
        if self.pending.len() > MAX_PENDING {
            self.pending.remove(0);
        }
        self.flush_pending();
        self.update_view();
    }

    /// Size of this core's own file (0 when there is none).
    pub fn own_file_len(&self) -> u64 {
        self.own_id
            .as_ref()
            .and_then(|id| fs::metadata(self.context_dir().join(id)).ok())
            .map_or(0, |m| m.len())
    }

    /// Compacts this core's own file on a worker thread (see `compact`); does nothing while an
    /// earlier compaction still runs.
    pub fn compact_in_background(&mut self, keep: HashSet<Key>) {
        let Some(id) = self.own_id.clone() else {
            return;
        };
        if let Some(h) = &self.compaction {
            if !h.is_finished() {
                return;
            }
        }
        if let Some(h) = self.compaction.take() {
            let _ = h.join();
        }
        let root = self.dir.root.clone();
        self.compaction = Some(std::thread::spawn(move || {
            let _ = compact(&root, &id, &keep);
        }));
    }

    /// Loads a secret that appeared, or drops everything when the key file went or changed.
    fn check_secret(&mut self) {
        let stamp = fs::metadata(self.dir.root.join(SECRET_FILE))
            .ok()
            .map(|m| (m.dev(), m.ino()));
        if self.secret.is_some() && stamp != self.key_stamp {
            self.secret = None;
            self.key_stamp = None;
            self.own_id = None;
            self.files.clear();
            self.base = ContextIndex::default();
            self.view = None;
            self.pending.clear();
            self.rebuild = None;
        }
        if self.secret.is_none() && stamp.is_some() {
            if let Ok(Some(secret)) = Secret::load(&self.dir.root) {
                self.own_id = self
                    .history_path
                    .as_deref()
                    .map(|p| own_file_id(&secret, p));
                self.secret = Some(secret);
                self.key_stamp = stamp;
            }
        }
    }

    fn flush_pending(&mut self) {
        let Some(id) = &self.own_id else {
            return;
        };
        if !self.pending.is_empty() && append(&self.dir.root, id, &self.pending).is_ok() {
            self.pending.clear();
        }
    }

    fn update_view(&mut self) {
        self.view = (!self.pending.is_empty()).then(|| {
            let mut v = self.base.clone();
            for r in &self.pending {
                v.add(r);
            }
            v
        });
    }

    /// Reads growth (and new files) inline; a file replaced, shrunk or removed starts a full
    /// re-read on a worker thread, swapped in by a later refresh.
    fn read_changes(&mut self) {
        let dir = self.context_dir();
        let ids = context_ids(&dir);
        let mut rebuild = self.files.keys().any(|id| !ids.contains(id));
        for id in &ids {
            let path = dir.join(id);
            let Ok(meta) = fs::metadata(&path) else {
                rebuild = true;
                continue;
            };
            match self.files.get_mut(id) {
                None => {
                    let mut state = FileState {
                        dev: meta.dev(),
                        ino: meta.ino(),
                        consumed: 0,
                        tail: Vec::new(),
                    };
                    if read_into(&path, &mut state, &mut self.base).is_ok() {
                        self.files.insert(id.clone(), state);
                    }
                }
                Some(st)
                    if (meta.dev(), meta.ino()) != (st.dev, st.ino) || meta.len() < st.consumed =>
                {
                    rebuild = true;
                }
                Some(st) if meta.len() > st.consumed => {
                    if read_into(&path, st, &mut self.base).is_err() {
                        rebuild = true;
                    }
                }
                Some(_) => {}
            }
        }
        if rebuild {
            self.rebuild = Some(std::thread::spawn(move || load_all(&dir)));
        }
    }
}

/// Compacts `<root>/context/<id>` under its lock taken exclusively (busy: returns `Ok(false)`
/// at once): keeps the records whose key is in `keep`; if they are still over `CAP`, drops the
/// oldest (by time, then file order) down to `TRIM_TO`; writes the result to a temporary file
/// (`0600`) and renames it over the old one. `Ok(true)` when the file was rewritten.
pub fn compact(root: &Path, id: &str, keep: &HashSet<Key>) -> io::Result<bool> {
    let dir = root.join(CONTEXT_DIR);
    let lock = open_lock(&dir, id)?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => return Ok(false),
        Err(TryLockError::Error(e)) => return Err(e),
    }
    let path = dir.join(id);
    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let all_lines = bytes
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .count();
    let mut kept: Vec<(Record, Vec<u8>)> = bytes
        .split(|&b| b == b'\n')
        .filter_map(decode)
        .filter(|r| keep.contains(&r.key))
        .map(|r| {
            let line = encode(&r);
            (r, line)
        })
        .collect();
    let mut total: u64 = kept.iter().map(|(_, l)| l.len() as u64).sum();
    let mut changed = kept.len() != all_lines || !bytes.ends_with(b"\n") && !bytes.is_empty();
    if total > CAP {
        let mut order: Vec<usize> = (0..kept.len()).collect();
        order.sort_by_key(|&i| (kept[i].0.time, i));
        let mut drop = vec![false; kept.len()];
        for i in order {
            if total <= TRIM_TO {
                break;
            }
            total -= kept[i].1.len() as u64;
            drop[i] = true;
        }
        let mut i = 0;
        kept.retain(|_| {
            i += 1;
            !drop[i - 1]
        });
        changed = true;
    }
    if !changed {
        return Ok(false);
    }
    let tmp = dir.join(format!("{id}.tmp-{}", std::process::id()));
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    let out: Vec<u8> = kept.into_iter().flat_map(|(_, l)| l).collect();
    let written = f
        .write_all(&out)
        .and_then(|()| f.sync_all())
        .and_then(|()| fs::rename(&tmp, &path));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written.map(|()| true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::record::{decode, Record};
    use crate::context::secret::Secret;
    use crate::context::Key;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn mode(p: &std::path::Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    fn rec(key: Key, folder: &str) -> Record {
        Record {
            key,
            folder: PathBuf::from(folder),
            status: 0,
            time: 1,
        }
    }

    #[test]
    fn context_file_never_holds_command_text() {
        let dir = tempfile::tempdir().unwrap();
        let s = Secret::from_bytes([7; 32]);
        let cmds = ["git push origin main", "ls", "secret-token-xyz"];
        let recs: Vec<_> = cmds.iter().map(|c| rec(s.key_of(c), "/p")).collect();
        append(dir.path(), "f", &recs).unwrap();
        let bytes = std::fs::read(dir.path().join("context/f")).unwrap();
        for c in cmds {
            let found = bytes.windows(c.len()).any(|w| w == c.as_bytes());
            assert!(!found, "{c} is in the file");
        }
        let lines: Vec<_> = bytes
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .collect();
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|l| decode(l).is_some()));
    }

    #[test]
    fn append_makes_0600_file_and_0700_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        append(&root, "f", &[rec(Key(1), "/p")]).unwrap();
        append(&root, "f", &[rec(Key(2), "/p")]).unwrap();
        assert_eq!(mode(&root), 0o700);
        assert_eq!(mode(&root.join("context")), 0o700);
        assert_eq!(mode(&root.join("context/f")), 0o600);
        let text = std::fs::read(root.join("context/f")).unwrap();
        assert_eq!(text.iter().filter(|&&b| b == b'\n').count(), 2);
    }

    #[test]
    fn append_with_the_lock_held_exclusively_is_busy() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("context")).unwrap();
        let lock = std::fs::File::create(root.join("context/f.lock")).unwrap();
        lock.try_lock().unwrap();
        assert!(matches!(
            append(root, "f", &[rec(Key(1), "/p")]),
            Err(AppendError::Busy)
        ));
        assert!(!root.join("context/f").exists());
        lock.unlock().unwrap();
        append(root, "f", &[rec(Key(1), "/p")]).unwrap();
    }

    #[test]
    fn own_file_id_is_the_keyed_hash_of_the_path() {
        let s = Secret::from_bytes([3; 32]);
        let a = own_file_id(&s, std::path::Path::new("/h/.zsh_history"));
        assert_eq!(a.len(), 32);
        assert_eq!(a, s.key_of("/h/.zsh_history").to_hex());
        assert_ne!(a, own_file_id(&s, std::path::Path::new("/h/.bash_history")));
    }

    #[test]
    fn forget_removes_context_and_key_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        Secret::load_or_create(&root).unwrap();
        append(&root, "f", &[rec(Key(1), "/p")]).unwrap();
        std::fs::write(root.join("other"), b"x").unwrap();
        assert!(forget(&root, true).unwrap());
        assert!(!root.join("context").exists());
        assert!(!root.join("key").exists());
        assert!(root.join("other").exists());
        assert!(!forget(&root, true).unwrap());
        std::fs::remove_file(root.join("other")).unwrap();
        // Only our files: the root goes too when asked.
        Secret::load_or_create(&root).unwrap();
        assert!(forget(&root, true).unwrap());
        assert!(!root.exists());
        // A plugin's root is kept even when empty.
        Secret::load_or_create(&root).unwrap();
        assert!(forget(&root, false).unwrap());
        assert!(root.exists());
        // Missing root: nothing to do.
        assert!(!forget(&dir.path().join("nope"), true).unwrap());
    }

    // ---- Task 2: reading, indexing, compacting ----

    use crate::context::{DataDir, DataSource};
    use std::collections::HashSet;
    use std::path::Path;
    use std::time::{Duration, Instant};

    const HIST: &str = "/h/.zsh_history";

    /// A data folder with a secret; the store opened on it for `HIST`.
    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        secret: Secret,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().join("data");
            let secret = Secret::load_or_create(&root).unwrap();
            Fixture {
                _dir: dir,
                root,
                secret,
            }
        }
        fn data(&self) -> DataDir {
            DataDir {
                root: self.root.clone(),
                source: DataSource::Xdg,
            }
        }
        fn open(&self) -> ContextStore {
            ContextStore::open(self.data(), Some(PathBuf::from(HIST)))
        }
        fn own(&self) -> String {
            own_file_id(&self.secret, Path::new(HIST))
        }
        fn key(&self, cmd: &str) -> Key {
            self.secret.key_of(cmd)
        }
        fn rec(&self, cmd: &str, folder: &str, status: i32, time: i64) -> Record {
            Record {
                key: self.key(cmd),
                folder: PathBuf::from(folder),
                status,
                time,
            }
        }
        fn file(&self, id: &str) -> PathBuf {
            self.root.join("context").join(id)
        }
        fn lines(&self, id: &str) -> Vec<Record> {
            std::fs::read(self.file(id))
                .unwrap_or_default()
                .split(|&b| b == b'\n')
                .filter(|l| !l.is_empty())
                .filter_map(decode)
                .collect()
        }
    }

    fn poll(mut f: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + Duration::from_secs(2);
        while Instant::now() < end {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        f()
    }

    #[test]
    fn open_reads_every_context_file() {
        let f = Fixture::new();
        let other = own_file_id(&f.secret, Path::new("/h/.bash_history"));
        append(&f.root, &f.own(), &[f.rec("make", "/p", 0, 10)]).unwrap();
        append(
            &f.root,
            &other,
            &[f.rec("make", "/q", 0, 30), f.rec("make", "/p", 1, 20)],
        )
        .unwrap();
        let s = f.open();
        let all = s.index().anywhere(f.key("make")).unwrap();
        assert_eq!((all.ok, all.failed, all.last_time), (2, 1, 30));
        let p = s.index().in_folder(Path::new("/p")).unwrap()[&f.key("make")];
        assert_eq!((p.ok, p.failed, p.last_time), (1, 1, 20));
    }

    #[test]
    fn no_secret_means_no_records_are_read() {
        let f = Fixture::new();
        append(&f.root, &f.own(), &[f.rec("make", "/p", 0, 10)]).unwrap();
        std::fs::remove_file(f.root.join("key")).unwrap();
        let s = f.open();
        assert!(s.secret().is_none());
        assert!(s.index().is_empty());
    }

    #[test]
    fn refresh_reads_growth_inline() {
        let f = Fixture::new();
        append(&f.root, &f.own(), &[f.rec("a", "/p", 0, 1)]).unwrap();
        let mut s = f.open();
        append(&f.root, &f.own(), &[f.rec("b", "/p", 0, 2)]).unwrap();
        s.refresh();
        assert_eq!(s.index().anywhere(f.key("b")).map(|t| t.ok), Some(1));
        assert_eq!(s.index().anywhere(f.key("a")).map(|t| t.ok), Some(1));
    }

    #[test]
    fn refresh_picks_up_a_new_file_and_a_new_secret() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        let data = DataDir {
            root: root.clone(),
            source: DataSource::Xdg,
        };
        let mut s = ContextStore::open(data, Some(PathBuf::from(HIST)));
        assert!(s.secret().is_none());
        // Another core makes the secret and a record.
        let secret = Secret::load_or_create(&root).unwrap();
        let id = own_file_id(&secret, Path::new("/h/.bash_history"));
        let r = Record {
            key: secret.key_of("ls"),
            folder: PathBuf::from("/p"),
            status: 0,
            time: 1,
        };
        append(&root, &id, &[r]).unwrap();
        assert!(poll(|| {
            s.refresh();
            s.index().anywhere(secret.key_of("ls")).is_some()
        }));
        assert!(s.secret() == Some(&secret));
    }

    #[test]
    fn refresh_rebuilds_after_a_rename_and_swaps_when_done() {
        let f = Fixture::new();
        append(&f.root, &f.own(), &[f.rec("old", "/p", 0, 1)]).unwrap();
        let mut s = f.open();
        let tmp = f.root.join("context/new");
        std::fs::write(&tmp, encode(&f.rec("new", "/p", 0, 2))).unwrap();
        std::fs::rename(&tmp, f.file(&f.own())).unwrap();
        s.refresh();
        // The old index serves until the rebuild is swapped in at a later refresh.
        assert!(s.index().anywhere(f.key("old")).is_some());
        assert!(poll(|| {
            s.refresh();
            s.index().anywhere(f.key("new")).is_some()
        }));
        assert!(s.index().anywhere(f.key("old")).is_none());
    }

    #[test]
    fn damaged_lines_are_skipped_and_a_torn_tail_is_read_once_completed() {
        let f = Fixture::new();
        let good = encode(&f.rec("good", "/p", 0, 1));
        let tail = encode(&f.rec("tail", "/p", 0, 2));
        let mut bytes = good.clone();
        bytes.extend_from_slice(b"junk line\n\xff\xfe\n2 future version\n");
        bytes.extend_from_slice(&tail[..10]);
        std::fs::create_dir_all(f.root.join("context")).unwrap();
        std::fs::write(f.file(&f.own()), &bytes).unwrap();
        let mut s = f.open();
        assert!(s.index().anywhere(f.key("good")).is_some());
        assert!(s.index().anywhere(f.key("tail")).is_none());
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(f.file(&f.own()))
            .unwrap();
        std::io::Write::write_all(&mut file, &tail[10..]).unwrap();
        s.refresh();
        assert_eq!(s.index().anywhere(f.key("tail")).map(|t| t.ok), Some(1));
        assert_eq!(s.index().anywhere(f.key("good")).map(|t| t.ok), Some(1));
    }

    #[test]
    fn record_without_history_path_writes_nothing() {
        let f = Fixture::new();
        let mut s = ContextStore::open(f.data(), None);
        s.record("make", Path::new("/p"), 0, 1);
        assert!(!f.root.join("context").exists());
    }

    #[test]
    fn record_creates_the_secret_on_first_use() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        let data = DataDir {
            root: root.clone(),
            source: DataSource::Xdg,
        };
        let mut s = ContextStore::open(data, Some(PathBuf::from(HIST)));
        assert!(s.secret().is_none());
        s.record("make", Path::new("/p"), 0, 1);
        let secret = Secret::load(&root).unwrap().unwrap();
        assert!(s.secret() == Some(&secret));
        s.refresh();
        assert_eq!(
            s.index().anywhere(secret.key_of("make")).map(|t| t.ok),
            Some(1)
        );
    }

    #[test]
    fn busy_append_stays_pending_and_lands_later() {
        let f = Fixture::new();
        let mut s = f.open();
        std::fs::create_dir_all(f.root.join("context")).unwrap();
        let lock = open_lock(&f.root.join("context"), &f.own()).unwrap();
        lock.try_lock().unwrap();
        s.record("make", Path::new("/p"), 0, 1);
        assert!(f.lines(&f.own()).is_empty());
        assert_eq!(s.index().anywhere(f.key("make")).map(|t| t.ok), Some(1));
        lock.unlock().unwrap();
        s.refresh();
        assert_eq!(f.lines(&f.own()).len(), 1);
        s.refresh();
        assert_eq!(s.index().anywhere(f.key("make")).map(|t| t.ok), Some(1));
    }

    #[test]
    fn compaction_drops_unknown_keys_and_trims_oldest() {
        let f = Fixture::new();
        let id = f.own();
        let mut recs = vec![f.rec("c-unknown", "/p", 0, 5)];
        // Old A records, enough to pass the cap.
        let one = encode(&f.rec("a", "/p", 0, 0)).len() as u64;
        let n = (CAP / one) + 100;
        recs.extend((0..n).map(|i| f.rec("a", "/p", 0, i as i64)));
        recs.push(f.rec("b", "/p", 0, i64::MAX - 1));
        recs.push(f.rec("a", "/p", 0, i64::MAX));
        append(&f.root, &id, &recs).unwrap();
        let keep: HashSet<Key> = [f.key("a"), f.key("b")].into();
        assert!(compact(&f.root, &id, &keep).unwrap());
        let len = std::fs::metadata(f.file(&id)).unwrap().len();
        assert!(len <= TRIM_TO, "{len}");
        let left = f.lines(&id);
        assert!(left.iter().all(|r| r.key != f.key("c-unknown")));
        assert!(left.iter().any(|r| r.key == f.key("b")));
        assert!(left
            .iter()
            .any(|r| r.key == f.key("a") && r.time == i64::MAX));
        assert!(
            left.iter().all(|r| r.key != f.key("a") || r.time != 0),
            "oldest went first"
        );
        assert_eq!(mode(&f.file(&id)), 0o600);
    }

    #[test]
    fn compaction_skips_when_locked() {
        let f = Fixture::new();
        let id = f.own();
        append(&f.root, &id, &[f.rec("gone", "/p", 0, 1)]).unwrap();
        let lock = open_lock(&f.root.join("context"), &id).unwrap();
        lock.try_lock_shared().unwrap();
        assert!(!compact(&f.root, &id, &HashSet::new()).unwrap());
        assert_eq!(f.lines(&id).len(), 1);
    }

    #[test]
    fn compaction_with_nothing_to_drop_leaves_the_file() {
        let f = Fixture::new();
        let id = f.own();
        append(&f.root, &id, &[f.rec("a", "/p", 0, 1)]).unwrap();
        let ino = std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(f.file(&id)).unwrap());
        assert!(!compact(&f.root, &id, &[f.key("a")].into()).unwrap());
        let after = std::os::unix::fs::MetadataExt::ino(&std::fs::metadata(f.file(&id)).unwrap());
        assert_eq!(ino, after);
    }

    #[test]
    fn appends_during_compaction_are_not_lost() {
        let f = Fixture::new();
        let id = f.own();
        let keep: HashSet<Key> = (0..2).map(|t| f.key(&format!("t{t}"))).collect();
        append(&f.root, &id, &[f.rec("drop-me", "/p", 0, 0)]).unwrap();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let compactor = {
            let (root, id, keep, stop) = (f.root.clone(), id.clone(), keep.clone(), stop.clone());
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = compact(&root, &id, &keep);
                }
            })
        };
        let writers: Vec<_> = (0..2)
            .map(|t| {
                let (root, id, key) = (f.root.clone(), id.clone(), f.key(&format!("t{t}")));
                std::thread::spawn(move || {
                    for i in 0..2000i64 {
                        let r = Record {
                            key,
                            folder: PathBuf::from("/p"),
                            status: 0,
                            time: i,
                        };
                        loop {
                            match append(&root, &id, std::slice::from_ref(&r)) {
                                Ok(()) => break,
                                Err(AppendError::Busy) => std::thread::yield_now(),
                                Err(AppendError::Io(e)) => panic!("{e}"),
                            }
                        }
                    }
                })
            })
            .collect();
        for w in writers {
            w.join().unwrap();
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        compactor.join().unwrap();
        let bytes = std::fs::read(f.file(&id)).unwrap();
        let lines: Vec<_> = bytes
            .split(|&b| b == b'\n')
            .filter(|l| !l.is_empty())
            .collect();
        assert!(lines.iter().all(|l| decode(l).is_some()), "a torn line");
        for t in 0..2 {
            let mut times: Vec<i64> = lines
                .iter()
                .filter_map(|l| decode(l))
                .filter(|r| r.key == f.key(&format!("t{t}")))
                .map(|r| r.time)
                .collect();
            times.sort();
            assert_eq!(times, (0..2000).collect::<Vec<_>>(), "writer {t}");
        }
    }

    #[test]
    fn oversized_file_is_compacted_by_the_first_compaction() {
        let f = Fixture::new();
        let id = f.own();
        let r = f.rec("a", "/some/longer/folder/name/here", 0, 1);
        let line = encode(&r);
        let n = (16 * 1024 * 1024) / line.len();
        std::fs::create_dir_all(f.root.join("context")).unwrap();
        std::fs::write(f.file(&id), line.repeat(n)).unwrap();
        let mut s = f.open();
        assert!(s.own_file_len() > CAP);
        s.compact_in_background([f.key("a")].into());
        assert!(poll(|| s.own_file_len() <= TRIM_TO));
    }
}
