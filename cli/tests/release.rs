//! What a release needs to agree on (M3): one version everywhere (`Cargo.toml`, the inter-crate
//! pins, `fuxx-plugin.conf`, the binary), the plugin manifest of design §8, and the files each
//! published crate carries (its README and both licence texts, byte-equal to the root's).
//! Read as plain lines: no TOML parser, no new dependency.

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The value of `key = value` or `key = "value"` on its own line; comments and other keys skipped.
fn field(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let line = line.trim();
        if line.starts_with('#') {
            return None;
        }
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    })
}

/// The lines of one `[section]` of a TOML file, up to the next section header.
fn section(text: &str, name: &str) -> String {
    let header = format!("[{name}]");
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            inside = t == header;
            continue;
        }
        if inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The `version = "=X"` pin inside a `[workspace.dependencies]` entry, without the `=`.
fn pin(deps: &str, krate: &str) -> Option<String> {
    let line = deps.lines().find(|l| {
        l.trim_start()
            .strip_prefix(krate)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    })?;
    let after = line.split("version").nth(1)?;
    let value = after.split('"').nth(1)?;
    Some(value.strip_prefix('=')?.to_string())
}

#[test]
fn versions_agree() {
    let cargo = read("Cargo.toml");
    let version = field(&section(&cargo, "workspace.package"), "version")
        .expect("[workspace.package] version");
    let deps = section(&cargo, "workspace.dependencies");
    assert_eq!(
        pin(&deps, "fuxx-completes-core").as_deref(),
        Some(version.as_str()),
        "fuxx-completes-core pin"
    );
    assert_eq!(
        pin(&deps, "fuxx-autosuggestions").as_deref(),
        Some(version.as_str()),
        "fuxx-autosuggestions pin"
    );
    assert_eq!(
        field(&read("fuxx-plugin.conf"), "version").as_deref(),
        Some(version.as_str()),
        "fuxx-plugin.conf version"
    );
    assert_eq!(env!("CARGO_PKG_VERSION"), version, "the binary's version");
}

#[test]
fn plugin_conf_matches_design() {
    let conf = read("fuxx-plugin.conf");
    assert_eq!(field(&conf, "format").as_deref(), Some("1"));
    assert_eq!(field(&conf, "id").as_deref(), Some("fuxx-completes"));
    assert_eq!(field(&conf, "bin").as_deref(), Some("bin"));
    assert_eq!(
        field(&conf, "zsh").as_deref(),
        Some("shell/fuxx-autosuggestions.plugin.zsh")
    );
    assert!(root()
        .join("adapters/zsh/fuxx-autosuggestions.plugin.zsh")
        .is_file());
}

#[test]
fn published_crates_carry_licences_and_readme() {
    for krate in ["core", "fuxx-autosuggestions", "cli"] {
        assert!(
            root().join(krate).join("README.md").is_file(),
            "{krate}/README.md"
        );
        assert_eq!(
            field(&read(&format!("{krate}/Cargo.toml")), "readme").as_deref(),
            Some("README.md"),
            "{krate}/Cargo.toml readme"
        );
        for licence in ["LICENSE-MIT", "LICENSE-APACHE"] {
            let copy = fs::read(root().join(krate).join(licence))
                .unwrap_or_else(|e| panic!("{krate}/{licence}: {e}"));
            assert!(
                copy == fs::read(root().join(licence)).unwrap(),
                "{krate}/{licence} differs from the root's"
            );
        }
    }
}

#[test]
fn fuxx_autocomplete_is_not_published() {
    assert_eq!(
        field(&read("fuxx-autocomplete/Cargo.toml"), "publish").as_deref(),
        Some("false")
    );
}
