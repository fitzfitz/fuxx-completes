//! The secret behind the keyed hash: 32 random bytes in `<root>/key` (`docs/design.md` §5.7).

//!
//! Created on the first record a core writes, never sent anywhere, removed by `forget`. A record
//! keyed with it can be read back only by a core that holds the same command text in its history.

use crate::context::Key;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

pub const SECRET_FILE: &str = "key";

#[derive(Clone, PartialEq, Eq)]
pub struct Secret([u8; 32]);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

impl Secret {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Secret(bytes)
    }

    /// The secret in `<root>/key`; `None` when there is none yet. A file that isn't 32 bytes
    /// is an error (never silently replaced: records keyed with it would be lost).
    pub fn load(root: &Path) -> io::Result<Option<Secret>> {
        match fs::read(root.join(SECRET_FILE)) {
            Ok(bytes) => <[u8; 32]>::try_from(bytes.as_slice())
                .map(|b| Some(Secret(b)))
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "key file is not 32 bytes")
                }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The secret in `<root>/key`, made first if there is none: 32 bytes from `/dev/urandom`,
    /// written to a temporary file and hard-linked into place, so another core racing to do the
    /// same either wins or reads a complete file. `root` is made `0700`, the file is `0600`.
    pub fn load_or_create(root: &Path) -> io::Result<Secret> {
        if let Some(s) = Self::load(root)? {
            return Ok(s);
        }
        make_private_dir(root)?;
        let mut bytes = [0u8; 32];
        fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        let tmp = root.join(format!("{SECRET_FILE}.tmp-{}", std::process::id()));
        let _ = fs::remove_file(&tmp);
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        let linked = f
            .write_all(&bytes)
            .and_then(|()| f.sync_all())
            .and_then(|()| fs::hard_link(&tmp, root.join(SECRET_FILE)));
        let _ = fs::remove_file(&tmp);
        match linked {
            Ok(()) => Ok(Secret(bytes)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Self::load(root)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "key file vanished")),
            Err(e) => Err(e),
        }
    }

    /// The keyed hash of `text` (BLAKE3 keyed mode), its first 16 bytes as a big-endian `u128`.
    pub fn key_of(&self, text: &str) -> Key {
        self.key_of_bytes(text.as_bytes())
    }

    pub fn key_of_bytes(&self, bytes: &[u8]) -> Key {
        let hash = blake3::keyed_hash(&self.0, bytes);
        let mut first = [0u8; 16];
        first.copy_from_slice(&hash.as_bytes()[..16]);
        Key(u128::from_be_bytes(first))
    }
}

/// Makes `dir` (and its parents) and sets it to `0700`.
pub(crate) fn make_private_dir(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(p: &std::path::Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn secret_is_created_once_with_0600_in_a_0700_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        assert!(Secret::load(&root).unwrap().is_none());
        let a = Secret::load_or_create(&root).unwrap();
        let b = Secret::load_or_create(&root).unwrap();
        assert!(a == b);
        assert_eq!(mode(&root), 0o700);
        assert_eq!(mode(&root.join("key")), 0o600);
        assert_eq!(std::fs::metadata(root.join("key")).unwrap().len(), 32);
        assert!(Secret::load(&root).unwrap().unwrap() == a);
    }

    #[test]
    fn secret_from_a_short_key_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("key"), b"short").unwrap();
        assert!(Secret::load(dir.path()).is_err());
    }

    #[test]
    fn key_of_is_keyed() {
        let a = Secret::from_bytes([1; 32]);
        let b = Secret::from_bytes([2; 32]);
        assert_ne!(a.key_of("ls"), b.key_of("ls"));
        assert_eq!(a.key_of("ls"), a.key_of("ls"));
        assert_ne!(a.key_of("ls"), a.key_of("ls "));
    }

    #[test]
    fn two_new_secrets_differ() {
        let d1 = tempfile::tempdir().unwrap();
        let d2 = tempfile::tempdir().unwrap();
        let a = Secret::load_or_create(d1.path()).unwrap();
        let b = Secret::load_or_create(d2.path()).unwrap();
        assert!(a != b);
    }
}
