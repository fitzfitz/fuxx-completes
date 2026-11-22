//! Every ecosystem in the mapping table has a fixture folder and is detected from it (M1b).
//! Fixtures are copied into a temp folder with a `.git` marker (git can't track a file named
//! `.git`), so the walk stops there and never sees the repo around them.

use fuxx_completes_core::projects::detect::detect;
use fuxx_completes_core::projects::PROJECTS;
use std::path::Path;
use std::time::{Duration, Instant};

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}

#[test]
fn every_ecosystem_has_a_fixture_and_is_detected() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/projects");
    for p in PROJECTS {
        let src = fixtures.join(p.id);
        assert!(src.is_dir(), "no fixture for {}", p.id);
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap().join(p.id);
        copy(&src, &root);
        std::fs::write(root.join(".git"), "").unwrap();
        let d = detect(&root, None, Instant::now() + Duration::from_secs(5));
        assert!(d.ids.contains(&p.id), "{}: detected {:?}", p.id, d.ids);
        if let Some(first) = p.commands.first() {
            assert!(
                d.candidates.iter().any(|c| c == first),
                "{}: no {first:?}",
                p.id
            );
        }
        for s in p.from {
            for t in s.templates {
                let want = t.replace("{}", "fx");
                assert!(
                    d.candidates.contains(&want),
                    "{} ({:?}): no {want:?} in {:?}",
                    p.id,
                    s.reader,
                    d.candidates
                );
            }
        }
    }
}
