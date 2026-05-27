//! Golden-file tests for `etna workload doc`. Each fixture under
//! `workloads/Test/DocGen/<case>/` contains an `etna.toml` plus committed
//! `expected_BUGS.md` / `expected_TASKS.md`. The test runs `generate_docs`
//! against the manifest and asserts the rendered output matches the expected
//! files byte-for-byte.
//!
//! To update goldens after an intentional generator change:
//!   cargo run -- workload doc etna2/workloads/Test/DocGen/<case>
//!   mv <case>/BUGS.md <case>/expected_BUGS.md
//!   mv <case>/TASKS.md <case>/expected_TASKS.md

use std::path::Path;

use etna::{service::workload as wl_service, workload::WorkloadManifest};

fn fixture_dir(case: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("workloads")
        .join("Test")
        .join("DocGen")
        .join(case)
}

fn assert_golden(case: &str) {
    let dir = fixture_dir(case);
    let manifest = WorkloadManifest::read(&dir)
        .unwrap_or_else(|e| panic!("read manifest for '{}': {}", case, e));
    let out = wl_service::generate_docs(&manifest)
        .unwrap_or_else(|| panic!("fixture '{}' should produce docs", case));

    let expected_bugs =
        std::fs::read_to_string(dir.join("expected_BUGS.md")).expect("expected_BUGS.md");
    let expected_tasks =
        std::fs::read_to_string(dir.join("expected_TASKS.md")).expect("expected_TASKS.md");

    assert_eq!(
        out.bugs_md, expected_bugs,
        "BUGS.md golden mismatch for '{}' — generated output:\n{}",
        case, out.bugs_md
    );
    assert_eq!(
        out.tasks_md, expected_tasks,
        "TASKS.md golden mismatch for '{}' — generated output:\n{}",
        case, out.tasks_md
    );
}

#[test]
fn tinyvec_like_matches_golden() {
    assert_golden("tinyvec_like");
}

#[test]
fn patch_injection_matches_golden() {
    assert_golden("patch_injection");
}

#[test]
fn empty_tasks_no_ops() {
    let dir = fixture_dir("empty_tasks");
    let manifest = WorkloadManifest::read(&dir).expect("read empty manifest");
    assert!(
        wl_service::generate_docs(&manifest).is_none(),
        "manifest with no [[tasks]] must skip doc generation"
    );
}

#[test]
fn regeneration_is_idempotent() {
    // Generator output must not drift between runs — same manifest in, same
    // bytes out. Catches accidental non-determinism (hash-map iteration etc.).
    let dir = fixture_dir("tinyvec_like");
    let m = WorkloadManifest::read(&dir).unwrap();
    let a = wl_service::generate_docs(&m).unwrap();
    let b = wl_service::generate_docs(&m).unwrap();
    assert_eq!(a.bugs_md, b.bugs_md);
    assert_eq!(a.tasks_md, b.tasks_md);
}
