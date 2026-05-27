//! Integration tests for `etna workload check`. Uses tempdirs to synthesize
//! workloads with specific breakage, then calls `collect_findings` and
//! asserts the returned findings match expectations.

use std::fs;
use std::path::Path;

use etna::commands::workload::check::collect_findings;
use etna::workload::WorkloadManifest;

fn write_tree(dir: &Path, files: &[(&str, &str)]) {
    for (rel, body) in files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, body).unwrap();
    }
}

fn load(dir: &Path) -> WorkloadManifest {
    WorkloadManifest::read(dir).expect("parse manifest")
}

// Uses patch injection so the disk-level marauder check is a no-op for
// these fixtures — we're not validating marauders vs. on-disk here, just
// the manifest-driven name/doc/witness/property checks.
const BASE_MANIFEST: &str = r#"
name = "demo"
language = "rust"
strategies = ["demo_strategy"]

[[tasks]]
mutations = ["demo_mutation_abcdef0_1"]

[tasks.injection]
kind = "patch"
files = ["src/lib.rs"]
patch = "patches/demo.patch"

[[tasks.tasks]]
property = "DemoProperty"
witnesses = [{ test_fn = "witness_demo_case_one" }]
"#;

#[test]
fn clean_workload_has_no_findings() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_tree(
        dir,
        &[
            ("etna.toml", BASE_MANIFEST),
            (
                "src/etna.rs",
                "fn property_demo_property() {} fn witness_demo_case_one() {}",
            ),
            ("patches/demo.patch", "(empty)"),
        ],
    );
    let manifest = load(dir);
    let docs = etna::service::workload::generate_docs(&manifest).unwrap();
    fs::write(dir.join("BUGS.md"), &docs.bugs_md).unwrap();
    fs::write(dir.join("TASKS.md"), &docs.tasks_md).unwrap();

    let findings = collect_findings(&manifest, dir);
    assert!(
        findings.is_empty(),
        "expected clean workload, got findings: {:#?}",
        findings
    );
}

#[test]
fn flags_missing_witness_and_property() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_tree(
        dir,
        &[
            ("etna.toml", BASE_MANIFEST),
            ("src/etna.rs", "fn unrelated() {}"),
            ("patches/demo.patch", "(empty)"),
        ],
    );
    let manifest = load(dir);
    // Generate docs to match so that only property/witness findings fire.
    let docs = etna::service::workload::generate_docs(&manifest).unwrap();
    fs::write(dir.join("BUGS.md"), &docs.bugs_md).unwrap();
    fs::write(dir.join("TASKS.md"), &docs.tasks_md).unwrap();

    let findings = collect_findings(&manifest, dir);
    assert!(
        findings
            .iter()
            .any(|f| f.contains("property 'DemoProperty'")),
        "expected missing-property finding, got: {:#?}",
        findings
    );
    assert!(
        findings
            .iter()
            .any(|f| f.contains("witness 'witness_demo_case_one'")),
        "expected missing-witness finding, got: {:#?}",
        findings
    );
}

#[test]
fn flags_bad_variant_name() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let manifest_text = r#"
name = "demo"
language = "rust"
strategies = ["demo_strategy"]

[[tasks]]
mutations = ["NotAVariantName"]

[tasks.injection]
kind = "patch"
files = ["src/lib.rs"]
patch = "patches/demo.patch"

[[tasks.tasks]]
property = "DemoProperty"
"#;
    write_tree(
        dir,
        &[
            ("etna.toml", manifest_text),
            ("src/etna.rs", "fn property_demo_property() {}"),
            ("patches/demo.patch", "(empty)"),
        ],
    );
    let manifest = load(dir);
    let docs = etna::service::workload::generate_docs(&manifest).unwrap();
    fs::write(dir.join("BUGS.md"), &docs.bugs_md).unwrap();
    fs::write(dir.join("TASKS.md"), &docs.tasks_md).unwrap();

    let findings = collect_findings(&manifest, dir);
    assert!(
        findings
            .iter()
            .any(|f| f.contains("variant name 'NotAVariantName'")),
        "expected bad-variant-name finding, got: {:#?}",
        findings
    );
}

#[test]
fn flags_stale_bugs_md() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    write_tree(
        dir,
        &[
            ("etna.toml", BASE_MANIFEST),
            (
                "src/etna.rs",
                "fn property_demo_property() {} fn witness_demo_case_one() {}",
            ),
            ("patches/demo.patch", "(empty)"),
            ("BUGS.md", "# stale content\n"),
            ("TASKS.md", "# stale content\n"),
        ],
    );
    let manifest = load(dir);
    let findings = collect_findings(&manifest, dir);
    assert!(
        findings
            .iter()
            .any(|f| f.contains("BUGS.md is out of sync")),
        "expected stale-BUGS finding, got: {:#?}",
        findings
    );
    assert!(
        findings
            .iter()
            .any(|f| f.contains("TASKS.md is out of sync")),
        "expected stale-TASKS finding, got: {:#?}",
        findings
    );
}

#[test]
fn flags_missing_patch_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let manifest_text = r#"
name = "demo"
language = "rust"
strategies = ["demo_strategy"]

[[tasks]]
mutations = ["demo_patch_abcdef0_1"]

[tasks.injection]
kind = "patch"
files = ["src/lib.rs"]
patch = "patches/does_not_exist.patch"

[[tasks.tasks]]
property = "DemoProperty"
"#;
    write_tree(
        dir,
        &[
            ("etna.toml", manifest_text),
            ("src/etna.rs", "fn property_demo_property() {}"),
        ],
    );
    let manifest = load(dir);
    let docs = etna::service::workload::generate_docs(&manifest).unwrap();
    fs::write(dir.join("BUGS.md"), &docs.bugs_md).unwrap();
    fs::write(dir.join("TASKS.md"), &docs.tasks_md).unwrap();

    let findings = collect_findings(&manifest, dir);
    assert!(
        findings
            .iter()
            .any(|f| f.contains("patch file 'patches/does_not_exist.patch' does not exist")),
        "expected missing-patch finding, got: {:#?}",
        findings
    );
}
