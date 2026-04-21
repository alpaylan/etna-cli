//! Integration tests for the workload catalog: resolution by name, fetch
//! from a `file://` URL override, and the add path using a catalog name
//! instead of a bare URL. Each test plants its own `workloads-index.json`
//! under `ETNA_HOME` so nothing here hits the network.

mod common;

use common::TestEtna;
use etna::{
    manager::Manager,
    service::{experiment as exp_svc, types::CreateExperimentOptions, workload as wl_svc},
    workload_index::{looks_like_url, WorkloadIndex, INDEX_URL_ENV},
};
use serial_test::serial;

fn write_index(etna_home: &std::path::Path, body: &str) {
    std::fs::write(etna_home.join("workloads-index.json"), body)
        .expect("failed to write cached index");
}

fn tiny_index(name: &str, url: &str) -> String {
    format!(
        r#"{{
            "schema_version": 1,
            "entries": [
                {{
                    "name": "{name}",
                    "url": "{url}",
                    "language": "Test",
                    "description": "fixture",
                    "status": "stable",
                    "tags": []
                }}
            ]
        }}"#
    )
}

#[test]
#[serial]
fn cached_index_overrides_bundled() {
    let fx = TestEtna::new();
    write_index(fx.etna_home(), &tiny_index("fixture-wl", "https://example.com/x"));

    let idx = WorkloadIndex::load().expect("load should succeed");
    assert_eq!(idx.entries.len(), 1);
    let e = idx.resolve("fixture-wl").expect("resolves by name");
    assert_eq!(e.language, "Test");
    assert_eq!(e.url, "https://example.com/x");
}

#[test]
#[serial]
fn corrupt_cache_falls_back_to_bundled() {
    let fx = TestEtna::new();
    write_index(fx.etna_home(), "not json at all");

    let idx = WorkloadIndex::load().expect("load falls back to bundled on parse error");
    // Bundled index is non-empty and contains the real catalog entries.
    assert!(idx.resolve("bst-haskell").is_some());
}

#[test]
#[serial]
fn fetch_from_file_url_writes_cache() {
    let fx = TestEtna::new();
    let src_dir = tempfile::TempDir::new().unwrap();
    let src = src_dir.path().join("remote-index.json");
    std::fs::write(&src, tiny_index("remote-wl", "https://example.com/r")).unwrap();

    std::env::set_var(INDEX_URL_ENV, format!("file://{}", src.display()));
    let idx = WorkloadIndex::fetch().expect("fetch should succeed from file://");
    std::env::remove_var(INDEX_URL_ENV);

    assert_eq!(idx.entries.len(), 1);
    assert_eq!(idx.resolve("remote-wl").unwrap().url, "https://example.com/r");

    let cached = std::fs::read_to_string(fx.etna_home().join("workloads-index.json"))
        .expect("cache should be written");
    assert!(cached.contains("remote-wl"));
}

#[test]
#[serial]
fn add_workload_resolves_catalog_name() {
    let fx = TestEtna::new();

    // Plant a workload repo on disk, then an index that points its name at
    // the filesystem path. `looks_like_url` recognizes abs paths as URLs,
    // so the index entry's `url` is what the service will pass to
    // `git submodule add` — which clones cleanly from a local path.
    let repo_path = fx.plant_workload_repo("T1");
    let index_body = tiny_index("t1-alias", repo_path.to_str().unwrap());
    write_index(fx.etna_home(), &index_body);

    // Register an experiment.
    let mut mgr = Manager::load().expect("Manager::load");
    let info = exp_svc::create_experiment(
        &mut mgr,
        CreateExperimentOptions {
            name: "catalog-exp".into(),
            path: Some(fx.scratch().to_path_buf()),
            overwrite: false,
        },
    )
    .expect("create_experiment");
    TestEtna::plant_marauder_config(&info.path);

    let mgr = Manager::load().unwrap();
    let meta = mgr.get_experiment("catalog-exp").expect("registered");

    let added = wl_svc::add_workload(&mgr, &meta, "t1-alias", None)
        .expect("add by catalog name should resolve via the planted index");

    // The workload's real name in etna.toml is "T1", not the catalog alias.
    assert_eq!(added.name, "T1");
    // Verify the submodule actually landed in the experiment tree.
    assert!(info.path.join("workloads/T1/etna.toml").exists());
}

#[test]
#[serial]
fn unknown_name_in_catalog_is_helpful_error() {
    let fx = TestEtna::new();
    write_index(fx.etna_home(), &tiny_index("something-else", "https://example.com/x"));

    let mut mgr = Manager::load().expect("Manager::load");
    let info = exp_svc::create_experiment(
        &mut mgr,
        CreateExperimentOptions {
            name: "err-exp".into(),
            path: Some(fx.scratch().to_path_buf()),
            overwrite: false,
        },
    )
    .expect("create_experiment");
    TestEtna::plant_marauder_config(&info.path);

    let mgr = Manager::load().unwrap();
    let meta = mgr.get_experiment("err-exp").unwrap();

    let err = wl_svc::add_workload(&mgr, &meta, "nonexistent", None)
        .expect_err("unknown catalog name should fail");
    let msg = format!("{err:#}");
    assert!(msg.contains("nonexistent"), "error should name the missing entry, got: {msg}");
    assert!(msg.contains("catalog"), "error should mention the catalog, got: {msg}");
}

#[test]
fn url_heuristic_matches_common_shapes() {
    assert!(looks_like_url("https://github.com/x/y"));
    assert!(looks_like_url("git@github.com:org/repo.git"));
    assert!(looks_like_url("/abs/path"));
    assert!(!looks_like_url("bst-haskell"));
    assert!(!looks_like_url(""));
}
