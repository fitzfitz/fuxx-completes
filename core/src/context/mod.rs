//! The context file (`docs/design.md` §5.7).

//!
//! A small local file of metadata per command the shell saved: a keyed hash of the command text,
//! the folder it ran in, its exit code and the time. Never the command text. It lives in the data
//! folder (`data_dir`), is read by every core and wiped by `fuxx-completes forget`.

pub mod record;
pub mod secret;
pub mod store;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// A keyed hash of a command's text, truncated to 128 bits (`Secret::key_of`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Key(pub u128);

impl Key {
    /// 32 lowercase hex digits.
    pub fn to_hex(&self) -> String {
        format!("{:032x}", self.0)
    }

    /// The inverse of `to_hex`: exactly 32 lowercase hex digits.
    pub fn from_hex(s: &str) -> Option<Key> {
        if s.len() != 32
            || !s
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return None;
        }
        u128::from_str_radix(s, 16).ok().map(Key)
    }
}

/// Where the data folder's path came from (`doctor` shows it).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DataSource {
    /// `FUXX_PLUGIN_DATA`: fuxx installed us as a plugin (design §8).
    PluginData,
    /// `$XDG_DATA_HOME/fuxx-completes`.
    Xdg,
    /// `$HOME/.local/share/fuxx-completes`.
    Home,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DataDir {
    pub root: PathBuf,
    pub source: DataSource,
}

/// The data folder (design §5.7, §8): `FUXX_PLUGIN_DATA` itself, else
/// `$XDG_DATA_HOME/fuxx-completes`, else `$HOME/.local/share/fuxx-completes`. A value that is
/// empty or not absolute counts as unset; with none usable there is no data folder.
pub fn data_dir(
    plugin_data: Option<&OsStr>,
    xdg_data_home: Option<&OsStr>,
    home: Option<&OsStr>,
) -> Option<DataDir> {
    if let Some(p) = absolute(plugin_data) {
        return Some(DataDir {
            root: p.to_path_buf(),
            source: DataSource::PluginData,
        });
    }
    if let Some(p) = absolute(xdg_data_home) {
        return Some(DataDir {
            root: p.join("fuxx-completes"),
            source: DataSource::Xdg,
        });
    }
    absolute(home).map(|h| DataDir {
        root: h.join(".local/share/fuxx-completes"),
        source: DataSource::Home,
    })
}

fn absolute(v: Option<&OsStr>) -> Option<&Path> {
    v.map(Path::new).filter(|p| p.is_absolute())
}

/// `data_dir` from this process's environment.
pub fn data_dir_from_env() -> Option<DataDir> {
    let var = std::env::var_os;
    data_dir(
        var("FUXX_PLUGIN_DATA").as_deref(),
        var("XDG_DATA_HOME").as_deref(),
        var("HOME").as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::Path;

    fn os(s: &str) -> Option<&OsStr> {
        Some(OsStr::new(s))
    }

    #[test]
    fn data_dir_prefers_plugin_data_then_xdg_then_home() {
        let d = data_dir(os("/p"), os("/x"), os("/h")).unwrap();
        assert_eq!(
            (d.root.as_path(), d.source),
            (Path::new("/p"), DataSource::PluginData)
        );
        let d = data_dir(None, os("/x"), os("/h")).unwrap();
        assert_eq!(
            (d.root.as_path(), d.source),
            (Path::new("/x/fuxx-completes"), DataSource::Xdg)
        );
        let d = data_dir(None, None, os("/h")).unwrap();
        assert_eq!(
            (d.root.as_path(), d.source),
            (
                Path::new("/h/.local/share/fuxx-completes"),
                DataSource::Home
            )
        );
        // Empty and relative count as unset.
        let d = data_dir(os(""), os("rel"), os("/h")).unwrap();
        assert_eq!(d.source, DataSource::Home);
        assert_eq!(data_dir(None, None, None), None);
        assert_eq!(data_dir(None, None, os("rel")), None);
    }

    #[test]
    fn key_hex_round_trips() {
        let k = Key(0x0123_4567_89ab_cdef_0011_2233_4455_6677);
        assert_eq!(k.to_hex(), "0123456789abcdef0011223344556677");
        assert_eq!(Key::from_hex(&k.to_hex()), Some(k));
        assert_eq!(Key::from_hex("0123"), None);
        assert_eq!(Key::from_hex("0123456789ABCDEF0011223344556677"), None);
        assert_eq!(Key::from_hex("g123456789abcdef0011223344556677"), None);
    }
}
