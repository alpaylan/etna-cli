//! Failure-path coverage for the service layer: missing workload, invalid
//! mode JSON in a hand-written test file, and run cancellation via the
//! shared cancel flag.

mod common;

use std::{
    path::Path,
    sync::{Arc, RwLock},
    thread,
    time::Duration,
};

use common::TestEtna;
use etna::{
    experiment::Mode,
    manager::Manager,
    service::{
        experiment as exp_svc,
        types::{CreateExperimentOptions, RunExperimentOptions},
        workload as wl_svc,
    },
};
use serial_test::serial;

fn fresh(name: &str) -> (TestEtna, std::path::PathBuf) {
    let fx = TestEtna::new();
    let mut mgr = Manager::load().expect("Manager::load");
    let info = exp_svc::create_experiment(
        &mut mgr,
        CreateExperimentOptions {
            name: name.into(),
            path: Some(fx.scratch().to_path_buf()),
            overwrite: false,
        },
    )
    .expect("create_experiment");
    TestEtna::plant_marauder_config(&info.path);
    (fx, info.path)
}

fn exp_name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
#[serial]
fn missing_workload_returns_error() {
    let (fx, exp) = fresh("exp_missing");
    let mgr = Manager::load().expect("Manager::load");
    let meta = mgr.get_experiment(&exp_name(&exp)).unwrap();

    let bogus = fx.etna_home().join("does-not-exist");
    let err = wl_svc::add_workload(&mgr, &meta, bogus.to_str().unwrap(), None)
        .err()
        .expect("add_workload should fail for a missing workload");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("does-not-exist") || msg.contains("clone") || msg.contains("not found"),
        "unexpected error: {msg}"
    );
}

#[test]
#[serial]
fn invalid_mode_json_fails_run() {
    let (fx, exp) = fresh("exp_badmode");

    {
        let mgr = Manager::load().expect("Manager::load");
        let meta = mgr.get_experiment(&exp_name(&exp)).unwrap();
        let t1 = fx.plant_workload_repo("T1");
        wl_svc::add_workload(&mgr, &meta, t1.to_str().unwrap(), None).expect("add_workload");
    }

    // Hand-write a tests/*.json with a bogus Mode discriminant — the file
    // parses as JSON but not as a valid `Vec<Test>`, so run_experiment's
    // per-test read should bail.
    let bogus = r#"[
      {
        "language": "Test",
        "workload": "T1",
        "trials": 1,
        "timeout": 10.0,
        "mutations": [],
        "mode": "NotARealMode",
        "tasks": []
      }
    ]"#;
    std::fs::write(exp.join("tests").join("bogus.json"), bogus).unwrap();

    let mgr = Manager::load().expect("Manager::load");
    let err = exp_svc::run_experiment(
        mgr,
        RunExperimentOptions {
            experiment_name: exp_name(&exp),
            tests: vec!["bogus".into()],
            short_circuit: false,
            parallel: false,
            params: vec![],
        },
        None,
    )
    .err()
    .expect("run_experiment should fail on bogus mode JSON");
    let msg = format!("{err:?}");
    assert!(
        msg.to_lowercase().contains("mode")
            || msg.to_lowercase().contains("parse")
            || msg.to_lowercase().contains("deserialize")
            || msg.to_lowercase().contains("invalid"),
        "unexpected error on bogus mode: {msg}"
    );
}

#[test]
#[serial]
fn cancel_flag_aborts_run() {
    let (fx, exp) = fresh("exp_cancel");

    {
        let mgr = Manager::load().expect("Manager::load");
        let meta = mgr.get_experiment(&exp_name(&exp)).unwrap();
        let t1 = fx.plant_workload_repo("T1");
        wl_svc::add_workload(&mgr, &meta, t1.to_str().unwrap(), None).expect("add_workload");
        // A Solve test with a larger trial count so we can set the flag
        // mid-flight — the driver checks the flag between trials.
        exp_svc::create_test(
            &mgr,
            &meta,
            "t",
            "T1",
            /*trials*/ 50,
            /*timeout*/ 10.0,
            Mode::Solve,
            vec![],
        )
        .expect("create_test");
    }

    let flag = Arc::new(RwLock::new(false));
    let flag_flipper = flag.clone();
    let exp_name_s = exp_name(&exp);

    let t = thread::spawn(move || {
        let mgr = Manager::load().expect("Manager::load");
        exp_svc::run_experiment(
            mgr,
            RunExperimentOptions {
                experiment_name: exp_name_s,
                tests: vec!["t".into()],
                short_circuit: false,
                parallel: false,
                params: vec![],
            },
            Some(flag),
        )
    });

    // Give the run a beat to start, then flip.
    thread::sleep(Duration::from_millis(50));
    *flag_flipper.write().unwrap() = true;

    let result = t.join().expect("run thread panicked");
    let err = result
        .err()
        .expect("run_experiment should report cancellation");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("cancel"),
        "expected 'cancel' in err, got: {msg}"
    );
}

/// Shared body for the hung-subprocess tests: build a T1→T2 cross experiment
/// whose fixture harness hangs (marker chosen by `property`), run it with a
/// 2s timeout, and require the run to terminate and record the outcome.
fn run_cross_with_hang(exp_label: &str, property: &str, expected_in_store: &str) {
    let (fx, exp) = fresh(exp_label);
    {
        let mgr = Manager::load().expect("Manager::load");
        let meta = mgr.get_experiment(&exp_name(&exp)).unwrap();
        let t1 = fx.plant_workload_repo("T1");
        let t2 = fx.plant_workload_repo("T2");
        wl_svc::add_workload(&mgr, &meta, t1.to_str().unwrap(), None).expect("add_workload T1");
        wl_svc::add_workload(&mgr, &meta, t2.to_str().unwrap(), None).expect("add_workload T2");
    }

    let test = format!(
        r#"[
      {{
        "workload": "T2",
        "trials": 1,
        "timeout": 2.0,
        "mutations": [],
        "mode": {{"Cross": {{"producer": {{"workload": "T1"}}, "consumer": {{"workload": "T2"}}}}}},
        "tasks": [{{"property": "{property}"}}]
      }}
    ]"#
    );
    std::fs::write(exp.join("tests").join("hang.json"), test).unwrap();

    let start = std::time::Instant::now();
    let mgr = Manager::load().expect("Manager::load");
    exp_svc::run_experiment(
        mgr,
        RunExperimentOptions {
            experiment_name: exp_name(&exp),
            tests: vec!["hang".into()],
            short_circuit: false,
            parallel: false,
            params: vec![],
        },
        None,
    )
    .expect("run_experiment should complete despite the hung subprocess");
    assert!(
        start.elapsed() < Duration::from_secs(30),
        "hung subprocess was not killed by the timeout (took {:?})",
        start.elapsed()
    );

    let store = std::fs::read_to_string(exp.join("store.jsonl")).unwrap();
    assert!(
        store.contains(expected_in_store),
        "expected {expected_in_store:?} in the store after a hung subprocess: {store}"
    );
}

/// Regression: the cross-mode producer sampler ran with no wall-clock cap, so
/// a wedged workload hung the harness forever.
#[test]
#[serial]
fn cross_hanging_sampler_times_out() {
    run_cross_with_hang("exp_hangs", "hang_sample", "timed_out");
}

/// Regression: cross-mode consumer steps ran with no wall-clock cap either.
#[test]
#[serial]
fn cross_hanging_consumer_aborts_with_timeout_error() {
    run_cross_with_hang("exp_hangc", "hang_test", "timed out");
}
