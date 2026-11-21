//! Writing and reading the context files under the data folder (`docs/design.md` §5.7).

//!
//! Each core appends to `context/<id>` for its own history file (`own_file_id`), and reads every
//! file in `context/`. A `context/<id>.lock` beside each file is taken shared by appends and
//! exclusive by a compaction, always with `try_lock`: nobody ever waits on it.

use crate::context::record::{encode, Record};
use crate::context::secret::{make_private_dir, Secret, SECRET_FILE};
use std::fs::{self, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

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
}
