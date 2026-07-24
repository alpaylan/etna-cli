//! End-to-end tests that drive the service layer against the `Test/T1` and
//! `Test/T2` fixtures. Each test exercises one `Mode` of the pipeline:
//! create_experiment → add_workload → create_test → run_experiment, and
//! asserts the metric landed in `store.jsonl` with the expected shape.

mod common;

use std::{
    io::{BufRead, BufReader},
    path::Path,
};

use common::TestEtna;
use etna::{
    experiment::{CexSource, InputSource, Mode, SampleCollect, Target},
    manager::Manager,
    service::{
        experiment as exp_svc,
        types::{CreateExperimentOptions, RunExperimentOptions},
        workload as wl_svc,
    },
};
use serial_test::serial;

fn create_experiment(fx: &TestEtna, name: &str) -> std::path::PathBuf {
    let mut mgr = Manager::load().expect("Manager::load");
    let info = exp_svc::create_experiment(
        &mut mgr,
        CreateExperimentOptions {
            name: name.to_string(),
            path: Some(fx.scratch().to_path_buf()),
            overwrite: false,
        },
    )
    .expect("create_experiment");
    TestEtna::plant_marauder_config(&info.path);
    info.path
}

fn add_workload(exp_path: &Path, url: &Path) {
    let mgr = Manager::load().expect("Manager::load");
    let meta = mgr
        .get_experiment(&exp_name(exp_path))
        .expect("experiment registered");
    wl_svc::add_workload(&mgr, &meta, url.to_str().unwrap(), None).expect("add_workload");
}

fn create_test(exp_path: &Path, test_name: &str, workload: &str, mode: Mode) {
    let mgr = Manager::load().expect("Manager::load");
    let meta = mgr
        .get_experiment(&exp_name(exp_path))
        .expect("experiment registered");
    exp_svc::create_test(
        &mgr,
        &meta,
        test_name,
        workload,
        /*trials*/ 1,
        /*timeout*/ 10.0,
        mode,
        vec![],
    )
    .expect("create_test");
}

fn run(exp_name: &str, test_name: &str) -> anyhow::Result<()> {
    let mgr = Manager::load().expect("Manager::load");
    exp_svc::run_experiment(
        mgr,
        RunExperimentOptions {
            experiment_name: exp_name.to_string(),
            tests: vec![test_name.to_string()],
            short_circuit: false,
            parallel: false,
            params: vec![],
        },
        None,
    )
}

fn exp_name(exp_path: &Path) -> String {
    exp_path.file_name().unwrap().to_string_lossy().into_owned()
}

fn read_metrics(exp_path: &Path) -> Vec<serde_json::Value> {
    let store = exp_path.join("store.jsonl");
    let f = std::fs::File::open(&store)
        .unwrap_or_else(|e| panic!("failed to open {}: {}", store.display(), e));
    BufReader::new(f)
        .lines()
        .filter_map(Result::ok)
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l).ok())
        .collect()
}

fn assert_metric_mode(metrics: &[serde_json::Value], mode: &str) {
    assert!(
        metrics.iter().any(|m| {
            m.get("data")
                .and_then(|d| d.get("mode"))
                .and_then(|v| v.as_str())
                == Some(mode)
        }),
        "no metric with mode={} in store. metrics={:#?}",
        mode,
        metrics
    );
}

#[test]
#[serial]
fn solve_mode_runs_and_logs_metric() {
    let fx = TestEtna::new();
    let exp = create_experiment(&fx, "exp_solve");
    let t1 = fx.plant_workload_repo("T1");
    add_workload(&exp, &t1);
    create_test(&exp, "t", "T1", Mode::Solve);

    run("exp_solve", "t").expect("run_experiment");

    let metrics = read_metrics(&exp);
    assert_metric_mode(&metrics, "solve");
    assert!(
        metrics.iter().any(|m| {
            m.get("data")
                .and_then(|d| d.get("status"))
                .and_then(|v| v.as_str())
                == Some("passed")
        }),
        "no 'passed' metric from the harness: {:#?}",
        metrics
    );
}

#[test]
#[serial]
fn sample_mode_runs() {
    let fx = TestEtna::new();
    let exp = create_experiment(&fx, "exp_sample");
    let t1 = fx.plant_workload_repo("T1");
    add_workload(&exp, &t1);
    create_test(
        &exp,
        "t",
        "T1",
        Mode::Sample {
            collect: SampleCollect::InputsAndStats,
        },
    );

    run("exp_sample", "t").expect("run_experiment");
    // Sample output is a JSON array, not per-line objects, so the store
    // picks up only the base context. Just assert the file exists and
    // parses.
    let _ = read_metrics(&exp);
}

#[test]
#[serial]
fn test_mode_counts_supplied_inputs() {
    let fx = TestEtna::new();
    let exp = create_experiment(&fx, "exp_test");
    let t1 = fx.plant_workload_repo("T1");
    add_workload(&exp, &t1);
    create_test(
        &exp,
        "t",
        "T1",
        Mode::Test {
            inputs: InputSource::Inline(vec!["a".into(), "b".into(), "c".into()]),
        },
    );

    run("exp_test", "t").expect("run_experiment");
    let metrics = read_metrics(&exp);
    assert_metric_mode(&metrics, "test");
    // The harness's `count_inputs` should report 3 for a JSON array of 3
    // items, which travels back as `tests` in the campaign-result.
    assert!(
        metrics.iter().any(|m| {
            m.get("data")
                .and_then(|d| d.get("tests"))
                .and_then(|v| v.as_u64())
                == Some(3)
        }),
        "expected tests=3 in some metric: {:#?}",
        metrics
    );
}

#[test]
#[serial]
fn shrink_mode_reports_shrink_count() {
    let fx = TestEtna::new();
    let exp = create_experiment(&fx, "exp_shrink");
    let t1 = fx.plant_workload_repo("T1");
    add_workload(&exp, &t1);
    create_test(
        &exp,
        "t",
        "T1",
        Mode::Shrink {
            counterexample: CexSource::Inline("xxxx".into()),
        },
    );

    run("exp_shrink", "t").expect("run_experiment");
    let metrics = read_metrics(&exp);
    assert_metric_mode(&metrics, "shrink");
    assert!(
        metrics.iter().any(|m| {
            m.get("data")
                .and_then(|d| d.get("shrinks"))
                .and_then(|v| v.as_u64())
                == Some(4)
        }),
        "expected shrinks=4 (len of 'xxxx'): {:#?}",
        metrics
    );
}

#[test]
#[serial]
fn cross_mode_producer_feeds_consumer() {
    let fx = TestEtna::new();
    let exp = create_experiment(&fx, "exp_cross");
    let t1 = fx.plant_workload_repo("T1");
    let t2 = fx.plant_workload_repo("T2");
    add_workload(&exp, &t1);
    add_workload(&exp, &t2);
    // Producer T1 emits inputs; consumer T2 runs `test --invert`, so a
    // property like "crash" (which T1 would find a bug for) does NOT fail
    // on T2 — the two workloads genuinely disagree, exercising Cross.
    create_test(
        &exp,
        "t",
        "T2",
        Mode::Cross {
            producer: Target {
                workload: "T1".into(),
            },
            consumer: Target {
                workload: "T2".into(),
            },
        },
    );

    run("exp_cross", "t").expect("run_experiment");
    let metrics = read_metrics(&exp);
    assert_metric_mode(&metrics, "cross");
    assert!(
        metrics.iter().any(|m| {
            m.get("data")
                .and_then(|d| d.get("producer_workload"))
                .and_then(|v| v.as_str())
                == Some("T1")
        }),
        "cross metric missing producer_workload: {:#?}",
        metrics
    );
    // Regression: batches that completed before the timeout contributed
    // nothing to passed/tests, so terminal cross metrics reported tests=1
    // regardless of how many consumer tests actually ran.
    assert!(
        metrics.iter().any(|m| {
            let data = m.get("data");
            let passed = data
                .and_then(|d| d.get("passed"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let tests = data
                .and_then(|d| d.get("tests"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            passed >= 1 && tests >= 2
        }),
        "terminal cross metric is missing completed-batch counts: {:#?}",
        metrics
    );
}
