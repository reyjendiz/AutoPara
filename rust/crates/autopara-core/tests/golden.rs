//! Parity with the Python implementation: `rust/fixtures/golden` (see its README).
//!
//! Until the parser lands (phase 2) this only checks that every input document has both of its
//! golden files and that they are readable; `cargo xtask golden` runs this target.

use std::fs;
use std::path::PathBuf;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/golden")
}

#[test]
fn every_document_has_its_golden_files() {
    let mut documents = 0;
    for entry in fs::read_dir(golden_dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "docx") {
            continue;
        }
        documents += 1;
        for suffix in ["parse.json", "scheduler.json"] {
            let golden = path.with_extension(suffix);
            let text = fs::read_to_string(&golden)
                .unwrap_or_else(|error| panic!("{}: {error}", golden.display()));
            serde_json::from_str::<serde_json::Value>(&text)
                .unwrap_or_else(|error| panic!("{}: {error}", golden.display()));
        }
    }
    assert_eq!(documents, 3, "expected three golden input documents");
}
