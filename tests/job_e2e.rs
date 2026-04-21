//! End-to-end coverage for `JobManager`: happy-path lifecycle wrapping a
//! real `run_experiment`, and a cancel round-trip that flips the shared
//! cancel flag while the driver is between trials.

mod common;

use std::{thread, time::Duration};

use common::TestEtna;
use etna::{
    experiment::Mode,
    manager::Manager,
    service::{
        experiment as exp_svc,
        job::{JobManager, JobStatus},
        types::{CreateExperimentOptions, RunExperimentOptions},
        workload as wl_svc,
    },
};
use serial_test::serial;

fn build_experiment(fx: &TestEtna, name: &str, trials: usize) -> std::path::PathBuf {
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

    let mgr = Manager::load().expect("Manager::load");
    let meta = mgr.get_experiment(name).expect("experiment registered");
    let t1 = fx.plant_workload_repo("T1");
    wl_svc::add_workload(&mgr, &meta, t1.to_str().unwrap(), None)
        .expect("add_workload");
    exp_svc::create_test(
        &mgr,
        &meta,
        "t",
        "T1",
        trials,
        /*timeout*/ 10.0,
        Mode::Solve,
        vec![],
    )
    .expect("create_test");

    info.path
}

#[test]
#[serial]
fn job_manager_tracks_a_real_experiment_run() {
    let fx = TestEtna::new();
    let _exp = build_experiment(&fx, "job_happy", 1);

    let jm = JobManager::new();
    let opts = RunExperimentOptions {
        experiment_name: "job_happy".into(),
        tests: vec!["t".into()],
        short_circuit: false,
        parallel: false,
        params: vec![],
    };
    let job_id = jm.create_experiment_run_job(&opts).expect("create job");

    assert_eq!(jm.get_job(&job_id).unwrap().status, JobStatus::Pending);

    jm.update_job_status(&job_id, JobStatus::Running).unwrap();
    let cancel_flag = jm.get_cancel_flag(&job_id).unwrap();

    let mgr = Manager::load().expect("Manager::load");
    exp_svc::run_experiment(mgr, opts, Some(cancel_flag)).expect("run_experiment");

    jm.update_job_status(&job_id, JobStatus::Completed).unwrap();

    let job = jm.get_job(&job_id).unwrap();
    assert_eq!(job.status, JobStatus::Completed);
    assert!(job.started_at.is_some(), "started_at not stamped");
    assert!(job.completed_at.is_some(), "completed_at not stamped");
}

#[test]
#[serial]
fn job_manager_cancels_a_live_run() {
    let fx = TestEtna::new();
    let _exp = build_experiment(&fx, "job_cancel", 50);

    let jm = JobManager::new();
    let opts = RunExperimentOptions {
        experiment_name: "job_cancel".into(),
        tests: vec!["t".into()],
        short_circuit: false,
        parallel: false,
        params: vec![],
    };
    let job_id = jm.create_experiment_run_job(&opts).expect("create job");
    jm.update_job_status(&job_id, JobStatus::Running).unwrap();
    let cancel_flag = jm.get_cancel_flag(&job_id).unwrap();

    let opts_clone = RunExperimentOptions {
        experiment_name: opts.experiment_name.clone(),
        tests: opts.tests.clone(),
        short_circuit: opts.short_circuit,
        parallel: opts.parallel,
        params: opts.params.clone(),
    };

    let t = thread::spawn(move || {
        let mgr = Manager::load().expect("Manager::load");
        exp_svc::run_experiment(mgr, opts_clone, Some(cancel_flag))
    });

    thread::sleep(Duration::from_millis(50));
    jm.cancel_job(&job_id).expect("cancel_job");

    let run_result = t.join().expect("run thread panicked");
    let err = run_result.err().expect("cancelled run should be Err");
    assert!(
        format!("{err:?}").to_lowercase().contains("cancel"),
        "expected cancel in error: {err:?}"
    );

    let job = jm.get_job(&job_id).unwrap();
    assert_eq!(job.status, JobStatus::Cancelled);
    assert!(jm.is_job_cancelled(&job_id).unwrap());
}
