//! The built-in readers (spec §5.4): each pulls names out of one small file's text, never a
//! value it would run. Every name returned passes `safe_name`.

use std::io::Read;
use std::path::Path;

/// Files larger than this are skipped (spec §4).
pub const MAX_FILE: u64 = 256 * 1024;

/// A name we may put into a command: 1–100 of `A-Za-z0-9-_.:/@+` (spec §5.4). Anything else is
/// dropped, never quoted.
pub fn safe_name(s: &str) -> bool {
    (1..=100).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:/@+".contains(&b))
}

/// The text of a regular file of at most `MAX_FILE` bytes (lossy UTF-8). Never opens anything
/// else: a FIFO or a device could block the reader.
pub fn read_small(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_FILE {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Keeps safe names, first occurrence only.
fn finish(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in names {
        if safe_name(&n) && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

// ---- JSON (and JSONC) ----

enum Json {
    Obj(Vec<(String, Json)>),
    Arr(Vec<Json>),
    Str(String),
    Other,
}

struct JsonParser<'a> {
    s: &'a [u8],
    i: usize,
    depth: usize,
}

impl JsonParser<'_> {
    fn ws(&mut self) {
        loop {
            while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            if self.s[self.i..].starts_with(b"//") {
                while self.i < self.s.len() && self.s[self.i] != b'\n' {
                    self.i += 1;
                }
            } else if self.s[self.i..].starts_with(b"/*") {
                match self.s[self.i + 2..].windows(2).position(|w| w == b"*/") {
                    Some(p) => self.i += p + 4,
                    None => self.i = self.s.len(),
                }
            } else {
                return;
            }
        }
    }

    fn string(&mut self) -> Option<String> {
        if self.s.get(self.i) != Some(&b'"') {
            return None;
        }
        self.i += 1;
        let mut out = Vec::new();
        loop {
            let b = *self.s.get(self.i)?;
            self.i += 1;
            match b {
                b'"' => return Some(String::from_utf8_lossy(&out).into_owned()),
                b'\\' => {
                    let e = *self.s.get(self.i)?;
                    self.i += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b't' => out.push(b'\t'),
                        b'r' => out.push(b'\r'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'u' => {
                            let hex = std::str::from_utf8(self.s.get(self.i..self.i + 4)?).ok()?;
                            self.i += 4;
                            let c = char::from_u32(u32::from_str_radix(hex, 16).ok()?)
                                .unwrap_or('\u{fffd}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                        }
                        other => out.push(other),
                    }
                }
                _ => out.push(b),
            }
        }
    }

    fn value(&mut self) -> Option<Json> {
        self.ws();
        self.depth += 1;
        if self.depth > 128 {
            return None;
        }
        let v = match *self.s.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.ws();
                    if self.s.get(self.i) == Some(&b'}') {
                        self.i += 1;
                        break;
                    }
                    let k = self.string()?;
                    self.ws();
                    (self.s.get(self.i) == Some(&b':')).then_some(())?;
                    self.i += 1;
                    let v = self.value()?;
                    items.push((k, v));
                    self.ws();
                    match self.s.get(self.i)? {
                        b',' => self.i += 1,
                        b'}' => {}
                        _ => return None,
                    }
                }
                Json::Obj(items)
            }
            b'[' => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.ws();
                    if self.s.get(self.i) == Some(&b']') {
                        self.i += 1;
                        break;
                    }
                    items.push(self.value()?);
                    self.ws();
                    match self.s.get(self.i)? {
                        b',' => self.i += 1,
                        b']' => {}
                        _ => return None,
                    }
                }
                Json::Arr(items)
            }
            b'"' => Json::Str(self.string()?),
            _ => {
                let start = self.i;
                while self.i < self.s.len() && !b",}] \t\r\n/".contains(&self.s[self.i]) {
                    self.i += 1;
                }
                (self.i > start).then_some(())?;
                Json::Other
            }
        };
        self.depth -= 1;
        Some(v)
    }
}

fn field<'j>(v: &'j Json, key: &str) -> Option<&'j Json> {
    match v {
        Json::Obj(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

/// `a.b`: the keys of the object there; `a[].k`: the string value of `k` in each element of the
/// array `a` (after any `x.` prefix). JSONC comments and trailing commas are fine.
pub fn json_keys(text: &str, path: &str) -> Vec<String> {
    let mut p = JsonParser {
        s: text.as_bytes(),
        i: 0,
        depth: 0,
    };
    let Some(root) = p.value() else {
        return Vec::new();
    };
    let mut cur = &root;
    let segs: Vec<&str> = path.split('.').collect();
    for (n, seg) in segs.iter().enumerate() {
        if let Some(arr) = seg.strip_suffix("[]") {
            let (Some(Json::Arr(items)), Some(k)) = (field(cur, arr), segs.get(n + 1)) else {
                return Vec::new();
            };
            return finish(items.iter().filter_map(|e| match field(e, k) {
                Some(Json::Str(s)) => Some(s.clone()),
                _ => None,
            }));
        }
        match field(cur, seg) {
            Some(v) => cur = v,
            None => return Vec::new(),
        }
    }
    match cur {
        Json::Obj(items) => finish(items.iter().map(|(k, _)| k.clone())),
        _ => Vec::new(),
    }
}

// ---- TOML ----

/// A bare or quoted key at the start of `line` (before `=`), or `None`.
fn toml_key(line: &str) -> Option<String> {
    let (k, _) = line.split_once('=')?;
    let k = k.trim();
    if let Some(q) = k.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
        return Some(q.to_string());
    }
    if let Some(q) = k.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        return Some(q.to_string());
    }
    let bare = !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    bare.then(|| k.to_string())
}

/// A TOML string value after `=` (`"…"` or `'…'`, no escapes needed for names).
fn toml_string(line: &str) -> Option<String> {
    let (_, v) = line.split_once('=')?;
    let v = v.trim();
    let q = v.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let rest = &v[1..];
    Some(rest[..rest.find(q)?].to_string())
}

/// `a.b`: the keys in table `[a.b]`; `a[].k`: `k = "…"` in each `[[a]]`.
pub fn toml_keys(text: &str, path: &str) -> Vec<String> {
    let (table, key) = match path.split_once("[].") {
        Some((t, k)) => (format!("[[{t}]]"), Some(k)),
        None => (format!("[{path}]"), None),
    };
    let mut inside = false;
    let mut out = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if l.starts_with('[') {
            let header: String = l
                .split('#')
                .next()
                .unwrap_or("")
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            inside = header == table;
            continue;
        }
        if !inside {
            continue;
        }
        match key {
            Some(k) => {
                if toml_key(l).as_deref() == Some(k) {
                    out.extend(toml_string(l));
                }
            }
            None => out.extend(toml_key(l)),
        }
    }
    finish(out)
}

// ---- Makefile ----

/// Rule targets at column 0: not variables (`:=`, `::=`, `?=`, `=`), not `.`-leading, no `%`
/// or `$`.
pub fn make_targets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with([' ', '\t', '#', '.']) {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue;
        };
        let (targets, rest) = line.split_at(colon);
        let after = rest.trim_start_matches(':');
        if after.starts_with('=') || targets.contains(['=', '?', '+']) {
            continue;
        }
        for t in targets.split_whitespace() {
            if !t.contains(['%', '$']) && !t.starts_with('.') && t != "export" {
                out.push(t.to_string());
            }
        }
    }
    finish(out)
}

// ---- justfile ----

/// Recipe headers at column 0 (`@` dropped), not `_`-leading; settings, aliases, variables,
/// imports, modules and attributes are skipped.
pub fn just_recipes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with([' ', '\t', '#', '[']) || line.contains(":=") {
            continue;
        }
        let first = line.split_whitespace().next().unwrap_or("");
        if ["alias", "set", "export", "import", "mod"].contains(&first) {
            continue;
        }
        let body = line.strip_prefix('@').unwrap_or(line);
        let name: String = body
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        let rest = &body[name.len()..];
        if name.is_empty() || name.starts_with('_') || !rest.contains(':') {
            continue;
        }
        let head = rest.split(':').next().unwrap_or("");
        if head.trim().is_empty() || head.starts_with(' ') {
            out.push(name);
        }
    }
    finish(out)
}

// ---- YAML ----

/// Block style only: the keys at the first child indent under a column-0 `key:`.
pub fn yaml_keys(text: &str, key: &str) -> Vec<String> {
    let mut lines = text.lines();
    let found = lines.by_ref().any(|l| {
        let l = l.split(" #").next().unwrap_or("").trim_end();
        l == format!("{key}:")
    });
    if !found {
        return Vec::new();
    }
    let mut indent = None;
    let mut out = Vec::new();
    for line in lines {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let n = line.len() - t.len();
        if n == 0 {
            break;
        }
        let want = *indent.get_or_insert(n);
        if n != want || t.starts_with('-') {
            continue;
        }
        if let Some((k, _)) = t.split_once(':') {
            let k = k.trim();
            let k = k
                .strip_prefix('"')
                .and_then(|r| r.strip_suffix('"'))
                .or_else(|| k.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))
                .unwrap_or(k);
            out.push(k.to_string());
        }
    }
    finish(out)
}

// ---- file names ----

/// Names in `names` ending in `ext`, without it, sorted (a listing has no order of its own).
pub fn file_names(names: &[String], ext: &str) -> Vec<String> {
    let mut found: Vec<String> = names
        .iter()
        .filter_map(|n| n.strip_suffix(ext))
        .map(str::to_string)
        .collect();
    found.sort();
    finish(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn safe_name_rules() {
        for ok in [
            "dev",
            "build:prod",
            "@scope/pkg",
            "a+b",
            "x.y_z-1",
            &"a".repeat(100),
        ] {
            assert!(safe_name(ok), "{ok:?}");
        }
        let long = "a".repeat(101);
        for bad in [
            "",
            long.as_str(),
            "rm -rf ~;",
            "a b",
            "x$y",
            "\"q\"",
            "a\u{1b}",
            "a\nb",
            "é",
        ] {
            assert!(!safe_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn json_keys_scripts_in_order() {
        let t = r#"{"name":"x","scripts":{"dev":"vite","build":"vite build","dev":"again"}}"#;
        assert_eq!(json_keys(t, "scripts"), ["dev", "build"]);
        assert!(json_keys(t, "nope").is_empty());
        assert!(json_keys("not json", "scripts").is_empty());
    }

    #[test]
    fn json_keys_nested_and_jsonc() {
        let t = r#"
        // a comment
        {
          /* block */ "tasks": {
            "start": { "nested": {"x": 1}, "arr": [1, "two", {"k": null}] },
            "with\"quote": "skipped: unsafe",
            "lint": "deno lint", // trailing
          },
          "deep": { "a": { "b": { "c": 1, "d": true } } },
        }"#;
        assert_eq!(json_keys(t, "tasks"), ["start", "lint"]);
        assert_eq!(json_keys(t, "deep.a.b"), ["c", "d"]);
    }

    #[test]
    fn json_keys_array_names() {
        let t = r#"{"configurePresets":[{"name":"debug"},{"hidden":true},{"name":"rel"},{"name":"a b"}]}"#;
        assert_eq!(json_keys(t, "configurePresets[].name"), ["debug", "rel"]);
    }

    #[test]
    fn json_keys_unsafe_names_dropped() {
        let t = r#"{"scripts":{"rm -rf ~;":"x","ok":"y","esc\u001b":"z","nl\n":"w"}}"#;
        assert_eq!(json_keys(t, "scripts"), ["ok"]);
    }

    #[test]
    fn toml_keys_table() {
        let t = "[project]\nname = \"x\"\n\n[project.scripts]\n# c\nserve = \"app:main\"\n\"quoted-key\" = \"m:f\"\nbad key = 1\n\n[tool.poetry.scripts]\nother = \"x\"\n";
        assert_eq!(toml_keys(t, "project.scripts"), ["serve", "quoted-key"]);
        assert_eq!(toml_keys(t, "tool.poetry.scripts"), ["other"]);
        assert_eq!(
            toml_keys("[alias]\nb = \"build\"\nt = 'test'\n", "alias"),
            ["b", "t"]
        );
    }

    #[test]
    fn toml_keys_array_of_tables() {
        let t = "[package]\nname = \"pkg\"\n\n[[bin]]\nname = \"tool\"\npath = \"src/t.rs\"\n\n[[bin]]\npath = \"x\"\nname = 'other'\n\n[dependencies]\nname = \"not-a-bin\"\n";
        assert_eq!(toml_keys(t, "bin[].name"), ["tool", "other"]);
    }

    #[test]
    fn make_targets_rules() {
        let t = "VAR := 1\nX ?= 2\n.PHONY: build test\nbuild:\n\tcc -o x\ntest: build\na b: c\n%.o: %.c\n$(OUT): x\nexport Y:=3\ninclude other.mk\nall:: one\n.hidden:\n  indented: no\n# c: no\nbuild:\n";
        assert_eq!(make_targets(t), ["build", "test", "a", "b", "all"]);
    }

    #[test]
    fn just_recipes_rules() {
        let t = "set shell := [\"bash\", \"-c\"]\nalias b := build\nx := \"1\"\nexport Y := \"2\"\nimport 'other.just'\nmod sub\n\n# comment\n[private]\nbuild:\n    cargo build\n@test arg='1':\n    cargo test\n_hidden:\n    echo\ndeploy target *flags: build\n    echo\n";
        assert_eq!(just_recipes(t), ["build", "test", "deploy"]);
    }

    #[test]
    fn yaml_keys_block() {
        let t = "version: '3'\n\ntasks:\n  # c\n  build:\n    cmds: []\n    nested: x\n  test:\n  \"quoted\":\n    deps: [build]\nvars:\n  notatask: 1\n";
        assert_eq!(yaml_keys(t, "tasks"), ["build", "test", "quoted"]);
        assert!(yaml_keys("tasks: {build: {}}\n", "tasks").is_empty());
        assert!(yaml_keys("other:\n  a: 1\n", "tasks").is_empty());
        assert_eq!(
            yaml_keys("services:\n    web:\n      image: x\n    db:\n", "services"),
            ["web", "db"]
        );
    }

    #[test]
    fn file_names_strip_the_extension_sorted() {
        let names = [
            "Zed.xcodeproj",
            "App.xcodeproj",
            "App.xcworkspace",
            "bad name.xcodeproj",
            "x.txt",
        ];
        let names: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        assert_eq!(file_names(&names, ".xcodeproj"), ["App", "Zed"]);
    }

    #[test]
    fn read_small_skips_big_and_non_regular() {
        let dir = tempfile::tempdir().unwrap();
        let small = dir.path().join("small");
        std::fs::write(&small, b"hello \xff").unwrap();
        assert_eq!(read_small(&small).as_deref(), Some("hello \u{fffd}"));
        let big = dir.path().join("big");
        std::fs::write(&big, vec![b'a'; 300 * 1024]).unwrap();
        assert_eq!(read_small(&big), None);
        assert_eq!(read_small(dir.path()), None);
        assert_eq!(read_small(&dir.path().join("missing")), None);
        let fifo = dir.path().join("fifo");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(made.success());
        let start = Instant::now();
        assert_eq!(read_small(&fifo), None);
        assert!(start.elapsed() < Duration::from_millis(100));
    }
}
