//! Which repo a folder belongs to (`docs/design.md` §5.4, M1d §4.3), so a habit in a repo
//! carries to its subfolders and worktrees. Nothing is run and nothing is written: `.git`
//! entries are looked at and a few small files read.

use crate::context::index::{ContextIndex, Tally};
use crate::context::Key;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Folders whose repo key is remembered (M1d spec §4.3).
pub const MAX_FOLDERS: usize = 256;
/// How long a remembered key is trusted before it is looked up again.
pub const MAX_AGE: Duration = Duration::from_secs(60);

/// The repo key of `folder`: the git folder shared by every worktree of one repo. `None`
/// outside a repo, for a folder that doesn't exist, or for anything unreadable.
/// The walk goes up at most `MAX_LEVELS` folders (as project detection), never looks at
/// `home` or `/`, and stops at the first `.git`: a folder is the key itself; a file `gitdir: X`
/// names the key, or, for a worktree (`X` under `<common>/worktrees/`), the common folder in
/// `X/commondir`. Keys are canonical paths.
pub fn repo_key(folder: &Path, home: Option<&Path>) -> Option<PathBuf> {
    if !folder.is_dir() {
        return None;
    }
    let mut dir = folder;
    for _ in 0..MAX_LEVELS {
        if Some(dir) == home || dir.parent().is_none() {
            return None;
        }
        let dotgit = dir.join(".git");
        match std::fs::metadata(&dotgit) {
            Ok(m) if m.is_dir() => return std::fs::canonicalize(&dotgit).ok(),
            Ok(_) => return key_from_file(dir, &dotgit),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
        dir = dir.parent()?;
    }
    None
}

/// How many folders the walk looks at, the start included.
const MAX_LEVELS: usize = 8;

/// A small text file, trimmed; `None` when it is missing, over 4 KB or not UTF-8.
fn small_text(path: &Path) -> Option<String> {
    let m = std::fs::metadata(path).ok()?;
    if !m.is_file() || m.len() > 4096 {
        return None;
    }
    Some(std::fs::read_to_string(path).ok()?.trim().to_owned())
}

/// The key a `.git` file names (`gitdir: X`, X relative to the folder holding the file).
fn key_from_file(dir: &Path, dotgit: &Path) -> Option<PathBuf> {
    let text = small_text(dotgit)?;
    let gitdir = dir.join(text.strip_prefix("gitdir:")?.trim());
    let in_worktrees = gitdir
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|n| n == "worktrees");
    if in_worktrees {
        if let Some(common) = small_text(&gitdir.join("commondir")) {
            return std::fs::canonicalize(gitdir.join(common)).ok();
        }
    }
    std::fs::canonicalize(gitdir).ok()
}

/// The folders whose files belong to the repo of `key`: the main worktree (the parent of a
/// `.git` key) and every linked worktree listed under `<key>/worktrees/*/gitdir`.
fn repo_roots(key: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if key.file_name().is_some_and(|n| n == ".git") {
        roots.extend(key.parent().map(Path::to_path_buf));
    }
    for entry in std::fs::read_dir(key.join("worktrees"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let Some(gitdir) = small_text(&entry.path().join("gitdir")) else {
            continue;
        };
        if let Some(root) = Path::new(&gitdir).parent() {
            roots.push(std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()));
        }
    }
    roots
}

/// Repo keys of folders, remembered for `MAX_AGE`, at most `MAX_FOLDERS` of them.
#[derive(Debug, Default)]
pub struct RepoCache {
    entries: HashMap<PathBuf, (Option<PathBuf>, Instant, u64)>,
    uses: u64,
}

impl RepoCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `repo_key`, remembered.
    pub fn key(&mut self, folder: &Path, home: Option<&Path>) -> Option<PathBuf> {
        self.uses += 1;
        let now = Instant::now();
        if let Some((key, at, used)) = self.entries.get_mut(folder) {
            if now.duration_since(*at) < MAX_AGE {
                *used = self.uses;
                return key.clone();
            }
        }
        let key = repo_key(folder, home);
        if !self.entries.contains_key(folder) && self.entries.len() >= MAX_FOLDERS {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, (_, _, used))| *used)
                .map(|(f, _)| f.clone());
            if let Some(f) = oldest {
                self.entries.remove(&f);
            }
        }
        self.entries
            .insert(folder.to_path_buf(), (key.clone(), now, self.uses));
        key
    }

    /// The tallies of the records in the other folders of `cwd`'s repo, merged per command;
    /// `None` when `cwd` is in no repo.
    pub fn tallies(
        &mut self,
        index: &ContextIndex,
        cwd: &Path,
        home: Option<&Path>,
    ) -> Option<HashMap<Key, Tally>> {
        let key = self.key(cwd, home)?;
        let roots = repo_roots(&key);
        let mut out: HashMap<Key, Tally> = HashMap::new();
        for (folder, tallies) in index.folders() {
            if folder == cwd || !roots.iter().any(|r| folder.starts_with(r)) {
                continue;
            }
            // A nested repo under one of the roots has a key of its own.
            if self.key(folder, home).as_ref() != Some(&key) {
                continue;
            }
            for (k, t) in tallies {
                let m = out.entry(*k).or_default();
                m.ok = m.ok.saturating_add(t.ok);
                m.failed = m.failed.saturating_add(t.failed);
                m.last_time = m.last_time.max(t.last_time);
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::record::Record;
    use std::fs;

    fn tmp() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = fs::canonicalize(d.path()).unwrap();
        (d, p)
    }

    /// `main/.git/` with a worktree `wt` whose `.git` file points back.
    fn with_worktree(root: &Path) -> (PathBuf, PathBuf) {
        let main = root.join("main");
        let wt = root.join("wt");
        let admin = main.join(".git/worktrees/wt");
        fs::create_dir_all(&admin).unwrap();
        fs::create_dir_all(main.join("a/b")).unwrap();
        fs::create_dir_all(wt.join("b")).unwrap();
        fs::write(
            admin.join("gitdir"),
            format!("{}\n", wt.join(".git").display()),
        )
        .unwrap();
        fs::write(admin.join("commondir"), "../..\n").unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
        (main, wt)
    }

    #[test]
    fn subfolders_share_the_repo_key() {
        let (_d, root) = tmp();
        let (main, _) = with_worktree(&root);
        let key = repo_key(&main, None);
        assert_eq!(key, Some(main.join(".git")));
        assert_eq!(repo_key(&main.join("a/b"), None), key);
    }

    #[test]
    fn worktrees_share_the_common_key() {
        let (_d, root) = tmp();
        let (main, wt) = with_worktree(&root);
        assert_eq!(repo_key(&wt, None), Some(main.join(".git")));
        assert_eq!(repo_key(&wt.join("b"), None), Some(main.join(".git")));
    }

    /// Review Focus 3.
    #[test]
    fn odd_layouts_have_no_or_their_own_key() {
        let (_d, root) = tmp();
        // A folder that no longer exists (a removed worktree).
        assert_eq!(repo_key(&root.join("gone"), None), None);
        // A `.git` at home never counts.
        let home = root.join("home");
        fs::create_dir_all(home.join(".git")).unwrap();
        fs::create_dir_all(home.join("x")).unwrap();
        assert_eq!(repo_key(&home.join("x"), Some(&home)), None);
        // A submodule: its own git folder under the super project's `.git/modules`.
        let sup = root.join("super");
        fs::create_dir_all(sup.join(".git/modules/m")).unwrap();
        fs::create_dir_all(sup.join("m")).unwrap();
        fs::write(sup.join("m/.git"), "gitdir: ../.git/modules/m\n").unwrap();
        assert_eq!(
            repo_key(&sup.join("m"), None),
            Some(sup.join(".git/modules/m"))
        );
        // A garbage `.git` file.
        let junk = root.join("junk");
        fs::create_dir_all(&junk).unwrap();
        fs::write(junk.join(".git"), "nonsense").unwrap();
        assert_eq!(repo_key(&junk, None), None);
        // An unreadable `.git` file.
        use std::os::unix::fs::PermissionsExt;
        let locked = root.join("locked");
        fs::create_dir_all(&locked).unwrap();
        fs::write(locked.join(".git"), "gitdir: ../main/.git\n").unwrap();
        fs::set_permissions(locked.join(".git"), fs::Permissions::from_mode(0o000)).unwrap();
        assert_eq!(repo_key(&locked, None), None);
        // A bare repo with a worktree.
        let bare = root.join("x.git");
        let admin = bare.join("worktrees/wt");
        fs::create_dir_all(&admin).unwrap();
        let wt = root.join("bare-wt");
        fs::create_dir_all(&wt).unwrap();
        fs::write(admin.join("commondir"), "../..\n").unwrap();
        fs::write(
            admin.join("gitdir"),
            format!("{}\n", wt.join(".git").display()),
        )
        .unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", admin.display())).unwrap();
        assert_eq!(repo_key(&wt, None), Some(bare.clone()));
    }

    #[test]
    fn tallies_merge_other_folders_of_the_repo() {
        let (_d, root) = tmp();
        let (main, wt) = with_worktree(&root);
        let other = root.join("other");
        fs::create_dir_all(&other).unwrap();
        let mut ix = ContextIndex::default();
        let add = |ix: &mut ContextIndex, k: u128, f: &Path, status: i32, time: i64| {
            ix.add(&Record {
                key: Key(k),
                folder: f.to_path_buf(),
                status,
                time,
            })
        };
        add(&mut ix, 1, &main.join("a"), 0, 5);
        add(&mut ix, 1, &wt.join("b"), 0, 9);
        add(&mut ix, 1, &wt, 2, 3);
        add(&mut ix, 1, &other, 0, 50);
        add(&mut ix, 2, &main, 0, 7);
        let mut cache = RepoCache::new();
        let t = cache.tallies(&ix, &main, None).unwrap();
        assert_eq!(
            t.get(&Key(1)),
            Some(&Tally {
                ok: 2,
                failed: 1,
                last_time: 9
            })
        );
        assert_eq!(t.get(&Key(2)), None, "the cwd's own records stay out");
        assert_eq!(cache.tallies(&ix, &other, None), None);
    }

    #[test]
    fn cache_is_bounded() {
        let mut cache = RepoCache::new();
        for i in 0..(MAX_FOLDERS + 1) {
            cache.key(Path::new(&format!("/zz-fuxx-missing/{i}")), None);
        }
        assert_eq!(cache.len(), MAX_FOLDERS);
        let (_d, root) = tmp();
        let (main, _) = with_worktree(&root);
        assert_eq!(cache.key(&main, None), Some(main.join(".git")));
    }
}
