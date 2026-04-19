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
    let (_fx, exp) = fresh("exp_missing");
    let mgr = Manager::load().expect("Manager::load");
    let meta = mgr.get_experiment(&exp_name(&exp)).unwrap();

    let err = wl_svc::add_workload(&mgr, &meta, "Test", "DoesNotExist")
        .err()
        .expect("add_workload should fail for a missing workload");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("DoesNotExist") || msg.contains("not found") || msg.contains("No such"),
        "unexpected error: {msg}"
    );
}

#[test]
#[serial]
fn invalid_mode_json_fails_run() {
    let (_fx, exp) = fresh("exp_badmode");

    {
        let mgr = Manager::load().expect("Manager::load");
        let meta = mgr.get_experiment(&exp_name(&exp)).unwrap();
        wl_svc::add_workload(&mgr, &meta, "Test", "T1").expect("add_workload");
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
    let (_fx, exp) = fresh("exp_cancel");

    {
        let mgr = Manager::load().expect("Manager::load");
        let meta = mgr.get_experiment(&exp_name(&exp)).unwrap();
        wl_svc::add_workload(&mgr, &meta, "Test", "T1").expect("add_workload");
        // A Solve test with a larger trial count so we can set the flag
        // mid-flight — the driver checks the flag between trials.
        exp_svc::create_test(
            &mgr,
            &meta,
            "t",
            "Test",
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
    let err = result.err().expect("run_experiment should report cancellation");
    let msg = format!("{err:?}").to_lowercase();
    assert!(
        msg.contains("cancel"),
        "expected 'cancel' in err, got: {msg}"
    );
}
