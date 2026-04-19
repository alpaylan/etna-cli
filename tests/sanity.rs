//! Smoke test that verifies the TestEtna fixture boots cleanly:
//! ETNA_HOME points at a tempdir, `setup` creates config.json +
//! experiments.json without any network traffic, and `Manager::load`
//! picks up the resulting state.

mod common;

use common::TestEtna;
use serial_test::serial;

#[test]
#[serial]
fn fixture_bootstraps_config() {
    let fx = TestEtna::new();

    let etna_home = fx.etna_home();
    assert!(etna_home.join("config.json").exists(), "config.json missing");
    assert!(
        etna_home.join("experiments.json").exists(),
        "experiments.json missing"
    );
    assert!(
        etna_home.join(".etna_cache").join(".git").exists(),
        ".etna_cache is not git-initialized"
    );
    assert!(
        etna_home.join(".etna_cache").join("workloads/Test/T1/steps.json").exists(),
        "Test/T1 fixture not copied into cache"
    );

    let mgr = etna::manager::Manager::load().expect("Manager::load failed");
    assert!(mgr.experiments.is_empty());
}
