//! Is a history entry a `cd`/`pushd` to a folder that certainly doesn't exist from here?
//! (`docs/design.md` §5.4, the user's decision 2026-11-13.)

use std::path::{Path, PathBuf};

/// Where the user is typing: the request's folder, the user's home and the shell's `cdpath`.
#[derive(Debug, Clone, Copy)]
pub struct Place<'a> {
    pub cwd: &'a Path,
    /// The core's own `$HOME`; `None` keeps every `~` target.
    pub home: Option<&'a Path>,
    /// The folders `cd` also searches for a relative target. `None` means unknown (an adapter
    /// that doesn't send it), so a relative target not found from `cwd` is kept.
    pub cdpath: Option<&'a [PathBuf]>,
}

/// True only when `command` is a `cd` or `pushd` whose one directory argument certainly isn't a
/// folder from `place`. Anything that can't be checked is false (kept): no argument, `-`, stack
/// refs (`-N`, `+N`), `~name`, anything with expansions, globs or braces, the two-argument form,
/// an option we don't know, redirections, unbalanced quotes.
pub fn certainly_missing(command: &str, place: &Place) -> bool {
    target(command).is_some_and(|arg| !is_folder(&arg.value, arg.tilde, place))
}

/// One shell word: its value after quote removal, and what its raw text held.
struct Word {
    value: String,
    /// Holds something we don't expand (`is_special`): `$`, a backtick, a glob or brace
    /// character, `!`, `=cmd`, `~` past the start.
    special: bool,
    /// Starts with an unquoted `~`.
    tilde: bool,
    /// The raw text had no quotes or backslashes.
    plain: bool,
    /// Starts with `NAME=` (unquoted): an assignment before the command.
    assign: bool,
}

/// The directory argument of a `cd`/`pushd`, or `None` when it isn't one or can't be checked.
fn target(command: &str) -> Option<Word> {
    let mut words = first_simple_command(command)?.into_iter().peekable();
    while words.peek().is_some_and(|w| w.assign) {
        words.next();
    }
    while words
        .peek()
        .is_some_and(|w| w.plain && (w.value == "builtin" || w.value == "command"))
    {
        words.next();
    }
    let cmd = words.next()?;
    if !(cmd.plain && (cmd.value == "cd" || cmd.value == "pushd")) {
        return None;
    }
    let mut args: Vec<Word> = words.collect();
    // Options: `-q -s -L -P` (and runs of them), then `--`.
    while let Some(first) = args.first() {
        let v = &first.value;
        if !first.plain || !v.starts_with('-') || v == "-" {
            break;
        }
        if v == "--" {
            args.remove(0);
            break;
        }
        if !v[1..].chars().all(|c| "qsLP".contains(c)) {
            return None; // `-N` or an option we don't know
        }
        args.remove(0);
    }
    if args.len() != 1 {
        return None; // no argument, or `cd old new`
    }
    let arg = args.pop()?;
    if arg.special || arg.value.is_empty() {
        return None;
    }
    if arg.plain && arg.value.starts_with('+') {
        return None; // a stack ref
    }
    if arg.plain && arg.value == "-" {
        return None;
    }
    Some(arg)
}

fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// Splits the first simple command into words: up to an unquoted `;`, `&`, `|` or newline.
/// `None` for what we don't parse: a redirection, parentheses, a comment, unbalanced quotes.
fn first_simple_command(command: &str) -> Option<Vec<Word>> {
    let mut words = Vec::new();
    let mut chars = command.chars().peekable();
    loop {
        while chars.peek().is_some_and(|&c| c == ' ' || c == '\t') {
            chars.next();
        }
        match chars.peek() {
            None | Some(';' | '&' | '|' | '\n') => return Some(words),
            Some('<' | '>' | '(' | ')' | '#') => return None,
            _ => {}
        }
        let mut w = Word {
            value: String::new(),
            special: false,
            tilde: chars.peek() == Some(&'~'),
            plain: true,
            assign: false,
        };
        let mut first = true;
        while let Some(&c) = chars.peek() {
            match c {
                ' ' | '\t' | ';' | '&' | '|' | '\n' => break,
                '<' | '>' | '(' | ')' => return None,
                _ => {}
            }
            chars.next();
            match c {
                '\\' => {
                    w.plain = false;
                    match chars.next() {
                        Some('\n') => {}
                        Some(e) => w.value.push(e),
                        None => return None,
                    }
                }
                '\'' => {
                    w.plain = false;
                    loop {
                        match chars.next()? {
                            '\'' => break,
                            q => {
                                w.special |= is_special(q, Quoting::Single);
                                w.value.push(q);
                            }
                        }
                    }
                }
                '"' => {
                    w.plain = false;
                    loop {
                        match chars.next()? {
                            '"' => break,
                            '\\' => match chars.next()? {
                                e @ ('$' | '`' | '"' | '\\') => w.value.push(e),
                                '\n' => {}
                                e => {
                                    w.value.push('\\');
                                    w.value.push(e);
                                }
                            },
                            q => {
                                w.special |= is_special(q, Quoting::Double);
                                w.value.push(q);
                            }
                        }
                    }
                }
                _ => {
                    if c == '=' && w.plain && !w.assign && is_name(&w.value) {
                        w.assign = true;
                    }
                    w.special |= is_special(c, Quoting::Bare { at_start: first });
                    w.value.push(c);
                }
            }
            first = false;
        }
        words.push(w);
    }
}

/// Where a character stands in a word.
#[derive(Clone, Copy)]
enum Quoting {
    /// Outside quotes; `at_start` for the word's first character.
    Bare {
        at_start: bool,
    },
    Double,
    Single,
}

/// Characters that make a word's value something we can't know without the shell (expansions,
/// globs, braces, history expansion, `=cmd`, `~` past the start). Kept broad on purpose: a false
/// "special" only keeps a candidate.
fn is_special(c: char, q: Quoting) -> bool {
    match q {
        Quoting::Single => false,
        Quoting::Double => matches!(c, '$' | '`' | '!'),
        Quoting::Bare { at_start } => {
            matches!(
                c,
                '$' | '`' | '*' | '?' | '[' | ']' | '{' | '}' | '!' | '^' | '#'
            ) || (c == '=' && at_start)
                || (c == '~' && !at_start)
        }
    }
}

/// Whether `path` (already unquoted) is a folder from `place`. Unknowns count as folders.
fn is_folder(path: &str, tilde: bool, place: &Place) -> bool {
    if tilde {
        let rest = match path.strip_prefix('~') {
            Some("") => "",
            Some(r) if r.starts_with('/') => r.trim_start_matches('/'),
            _ => return true, // `~name`: a named dir we can't see
        };
        return match place.home {
            Some(home) if home.is_absolute() => dir_at(&home.join(rest)),
            _ => true,
        };
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return dir_at(p);
    }
    if !place.cwd.is_absolute() {
        return true;
    }
    if dir_at(&place.cwd.join(p)) {
        return true;
    }
    match place.cdpath {
        None => true,
        Some(dirs) => dirs.iter().any(|d| {
            let base = if d.as_os_str().is_empty() {
                place.cwd.to_path_buf()
            } else {
                place.cwd.join(d)
            };
            dir_at(&base.join(p))
        }),
    }
}

/// A folder (or a symlink to one) at `p` taken physically, or with `..` taken lexically as zsh's
/// `cd` does by default (without CHASE_DOTS): either one counts.
fn dir_at(p: &Path) -> bool {
    let lexical = lexical(p);
    let is_dir = |p: &Path| std::fs::metadata(p).is_ok_and(|m| m.is_dir());
    is_dir(&lexical) || (lexical != p && is_dir(p))
}

/// `p` with `.` dropped and each `..` removing the component before it.
fn lexical(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(c);
                }
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    /// root/kerja/{proj/fuxx-cli, a b, file.txt, link -> proj, filelink -> file.txt,
    /// deeplink -> proj/fuxx-cli}, root/home/docs, root/cdp/target.
    struct Tree {
        _dir: TempDir,
        root: PathBuf,
    }

    impl Tree {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let root = fs::canonicalize(dir.path()).unwrap();
            let k = root.join("kerja");
            fs::create_dir_all(k.join("proj/fuxx-cli")).unwrap();
            fs::create_dir_all(k.join("a b")).unwrap();
            fs::write(k.join("file.txt"), "").unwrap();
            symlink(k.join("proj"), k.join("link")).unwrap();
            symlink(k.join("file.txt"), k.join("filelink")).unwrap();
            symlink(k.join("proj/fuxx-cli"), k.join("deeplink")).unwrap();
            fs::create_dir_all(root.join("home/docs")).unwrap();
            fs::create_dir_all(root.join("cdp/target")).unwrap();
            Tree { _dir: dir, root }
        }

        fn kerja(&self) -> PathBuf {
            self.root.join("kerja")
        }

        fn home(&self) -> PathBuf {
            self.root.join("home")
        }

        /// From `kerja`, with the home set and `cdpath` known to be empty.
        fn missing(&self, cmd: &str) -> bool {
            self.missing_from(&self.kerja(), cmd)
        }

        fn missing_from(&self, cwd: &Path, cmd: &str) -> bool {
            let home = self.home();
            certainly_missing(
                cmd,
                &Place {
                    cwd,
                    home: Some(&home),
                    cdpath: Some(&[]),
                },
            )
        }
    }

    #[test]
    fn relative_target_that_exists_is_kept() {
        let t = Tree::new();
        assert!(!t.missing("cd proj"));
        assert!(!t.missing("cd proj/fuxx-cli"));
        assert!(!t.missing("cd proj/"));
    }

    #[test]
    fn relative_target_that_is_missing_is_skipped() {
        let t = Tree::new();
        assert!(t.missing("cd fuxx-cli"));
        assert!(t.missing("cd proj/nope"));
    }

    #[test]
    fn the_same_target_is_kept_from_the_folder_that_holds_it() {
        let t = Tree::new();
        assert!(!t.missing_from(&t.kerja().join("proj"), "cd fuxx-cli"));
    }

    #[test]
    fn tilde_resolves_with_the_home() {
        let t = Tree::new();
        assert!(!t.missing("cd ~"));
        assert!(!t.missing("cd ~/docs"));
        assert!(t.missing("cd ~/nope"));
    }

    #[test]
    fn tilde_without_a_home_is_kept() {
        let t = Tree::new();
        let cwd = t.kerja();
        let place = Place {
            cwd: &cwd,
            home: None,
            cdpath: Some(&[]),
        };
        assert!(!certainly_missing("cd ~/nope", &place));
        assert!(!certainly_missing("cd ~", &place));
    }

    #[test]
    fn absolute_targets_are_checked() {
        let t = Tree::new();
        assert!(!t.missing(&format!("cd {}", t.kerja().join("proj").display())));
        assert!(t.missing(&format!("cd {}", t.root.join("nope").display())));
        assert!(!t.missing("cd /"));
    }

    #[test]
    fn dot_and_dot_dot() {
        let t = Tree::new();
        assert!(!t.missing("cd ."));
        assert!(!t.missing("cd .."));
        assert!(!t.missing("cd ./proj"));
        assert!(!t.missing_from(&t.kerja().join("proj"), "cd ../proj"));
        assert!(t.missing("cd ../nope"));
    }

    #[test]
    fn dot_dot_through_a_symlinked_folder_is_kept_either_way() {
        // From kerja/deeplink (-> proj/fuxx-cli): zsh's `cd ..` is logical by default (kerja),
        // but with CHASE_DOTS physical (proj). `../proj` exists logically only: kept.
        let t = Tree::new();
        let cwd = t.kerja().join("deeplink");
        assert!(!t.missing_from(&cwd, "cd ../proj"));
        // `../fuxx-cli` exists physically only (proj/fuxx-cli): kept too.
        assert!(!t.missing_from(&cwd, "cd ../fuxx-cli"));
        assert!(t.missing_from(&cwd, "cd ../nope"));
    }

    #[test]
    fn quoting_and_escapes_are_undone() {
        let t = Tree::new();
        assert!(!t.missing("cd 'a b'"));
        assert!(!t.missing("cd \"a b\""));
        assert!(!t.missing("cd a\\ b"));
        assert!(!t.missing("cd 'pro'j"));
        assert!(t.missing("cd 'x y'"));
        assert!(t.missing("cd x\\ y"));
        // A quoted `~` is a folder named `~`, not the home.
        assert!(t.missing("cd '~/docs'"));
    }

    #[test]
    fn what_cannot_be_checked_is_kept() {
        let t = Tree::new();
        for cmd in [
            "cd",
            "cd -",
            "cd -2",
            "cd +1",
            "cd ~name",
            "cd ~name/x",
            "cd $X",
            "cd ${X}/y",
            "cd \"$X\"",
            "cd `pwd`/x",
            "cd $(pwd)/x",
            "cd nope*",
            "cd nop?",
            "cd [ab]",
            "cd {a,b}",
            "cd =ls",
            "cd !$",
            "cd nope > out",
            "cd (nope)",
            "cd ''",
            "cd 'unterminated",
            "pushd",
            "pushd +2",
            "popd",
            "popd nope",
        ] {
            assert!(!t.missing(cmd), "{cmd:?} was skipped");
        }
    }

    #[test]
    fn options_are_skipped_before_the_target() {
        let t = Tree::new();
        assert!(t.missing("cd -q nope"));
        assert!(t.missing("cd -P nope"));
        assert!(t.missing("cd -L -s nope"));
        assert!(t.missing("cd -qP nope"));
        assert!(t.missing("cd -- nope"));
        assert!(!t.missing("cd -q proj"));
        assert!(!t.missing("cd -P -- proj"));
        // An option we don't know: can't say what it does.
        assert!(!t.missing("cd -x nope"));
        // After `--`, a word starting with `-` is the target.
        assert!(t.missing("cd -- -nope"));
        assert!(!t.missing("cd -q"));
    }

    #[test]
    fn two_argument_form_is_kept() {
        let t = Tree::new();
        assert!(!t.missing("cd old new"));
        assert!(!t.missing("cd -q old new"));
    }

    #[test]
    fn prefix_words_and_pushd() {
        let t = Tree::new();
        assert!(t.missing("builtin cd nope"));
        assert!(t.missing("command cd nope"));
        assert!(t.missing("FOO=1 cd nope"));
        assert!(t.missing("A=1 B='x y' builtin cd nope"));
        assert!(t.missing("pushd nope"));
        assert!(t.missing("pushd -q nope"));
        assert!(!t.missing("pushd proj"));
        assert!(!t.missing("builtin cd proj"));
    }

    #[test]
    fn other_commands_are_kept() {
        let t = Tree::new();
        for cmd in ["ls nope", "cdx nope", "echo cd nope", "git cd nope", "c"] {
            assert!(!t.missing(cmd), "{cmd:?} was skipped");
        }
    }

    #[test]
    fn only_the_first_simple_command_is_checked() {
        let t = Tree::new();
        assert!(t.missing("cd nope && make"));
        assert!(t.missing("cd nope; ls"));
        assert!(t.missing("cd nope|cat"));
        assert!(t.missing("  cd nope"));
        assert!(!t.missing("cd proj && make"));
        assert!(!t.missing("cd proj\nls"));
        assert!(!t.missing("ls; cd nope"));
    }

    #[test]
    fn symlink_to_a_folder_counts_as_a_folder() {
        let t = Tree::new();
        assert!(!t.missing("cd link"));
        assert!(!t.missing("cd link/fuxx-cli"));
    }

    #[test]
    fn a_file_named_like_the_target_is_not_a_folder() {
        let t = Tree::new();
        assert!(t.missing("cd file.txt"));
        assert!(t.missing("cd filelink"));
    }

    #[test]
    fn cdpath_folders_are_searched_for_a_relative_target() {
        let t = Tree::new();
        let cwd = t.kerja();
        let home = t.home();
        let cdpath = [t.root.join("cdp"), PathBuf::from("proj")];
        let place = Place {
            cwd: &cwd,
            home: Some(&home),
            cdpath: Some(&cdpath),
        };
        assert!(!certainly_missing("cd target", &place));
        // A relative cdpath entry is taken from the cwd: kerja/proj/fuxx-cli.
        assert!(!certainly_missing("cd fuxx-cli", &place));
        assert!(certainly_missing("cd nope", &place));
        // Absolute and `~` targets never use cdpath.
        assert!(certainly_missing(
            &format!("cd {}", t.root.join("target").display()),
            &place
        ));
    }

    #[test]
    fn unknown_cdpath_keeps_a_relative_target_not_found_here() {
        let t = Tree::new();
        let cwd = t.kerja();
        let home = t.home();
        let place = Place {
            cwd: &cwd,
            home: Some(&home),
            cdpath: None,
        };
        assert!(!certainly_missing("cd nope", &place));
        // Absolute and `~` targets are still checked.
        assert!(certainly_missing(
            &format!("cd {}", t.root.join("nope").display()),
            &place
        ));
        assert!(certainly_missing("cd ~/nope", &place));
    }

    #[test]
    fn a_relative_target_with_no_absolute_cwd_is_kept() {
        let place = Place {
            cwd: Path::new(""),
            home: None,
            cdpath: Some(&[]),
        };
        assert!(!certainly_missing("cd surely-not-here-zz", &place));
    }
}
