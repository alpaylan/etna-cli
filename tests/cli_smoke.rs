//! CLI smoke tests driving the compiled `etna` binary against the TestEtna
//! fixture. Covers `etna setup`, a full experiment pipeline (new → workload
//! add → create-test → run), `etna experiment visualize`, and `etna bash`.
//! Each test is `#[serial]` because TestEtna mutates process-global env and
//! CWD, which the child process inherits.

mod common;

use assert_cmd::Command;
use common::TestEtna;
use serial_test::serial;

fn etna() -> Command {
    Command::cargo_bin("etna").expect("compiled etna binary")
}

#[test]
#[serial]
fn setup_creates_config_files() {
    let fx = TestEtna::new_without_etna_setup();

    etna().arg("setup").assert().success();

    assert!(
        fx.etna_home().join("config.json").exists(),
        "config.json missing"
    );
    assert!(
        fx.etna_home().join("experiments.json").exists(),
        "experiments.json missing"
    );
}

#[test]
#[serial]
fn experiment_pipeline_produces_store() {
    let fx = TestEtna::new();

    etna().args(["experiment", "new", "toy"]).assert().success();

    let exp_path = fx.scratch().join("toy");
    TestEtna::plant_marauder_config(&exp_path);

    let t1 = fx.plant_workload_repo("T1");
    etna()
        .args(["workload", "add", "--experiment", "toy"])
        .arg(t1.to_str().unwrap())
        .assert()
        .success();

    etna()
        .args([
            "experiment",
            "create-test",
            "--name",
            "toy",
            "--workload",
            "T1",
            "--test",
            "solve_test",
            "--mode",
            "solve",
            "--trials",
            "1",
            "--timeout",
            "10",
        ])
        .assert()
        .success();

    etna()
        .args([
            "experiment",
            "run",
            "--name",
            "toy",
            "--tests",
            "solve_test",
        ])
        .assert()
        .success();

    let store = exp_path.join("store.jsonl");
    let contents = std::fs::read_to_string(&store)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", store.display(), e));
    assert!(
        !contents.trim().is_empty(),
        "store.jsonl should contain at least one metric line"
    );
}

#[test]
#[serial]
fn visualize_emits_figure_files() {
    let fx = TestEtna::new();

    etna().args(["experiment", "new", "viz"]).assert().success();
    let exp_path = fx.scratch().join("viz");
    TestEtna::plant_marauder_config(&exp_path);

    let t1 = fx.plant_workload_repo("T1");
    etna()
        .args(["workload", "add", "--experiment", "viz"])
        .arg(t1.to_str().unwrap())
        .assert()
        .success();
    etna()
        .args([
            "experiment",
            "create-test",
            "--name",
            "viz",
            "--workload",
            "T1",
            "--test",
            "solve_test",
            "--mode",
            "solve",
            "--trials",
            "1",
            "--timeout",
            "10",
        ])
        .assert()
        .success();
    etna()
        .args([
            "experiment",
            "run",
            "--name",
            "viz",
            "--tests",
            "solve_test",
        ])
        .assert()
        .success();

    etna()
        .args([
            "experiment",
            "visualize",
            "--name",
            "viz",
            "--figure",
            "smoke",
            "--tests",
            "solve_test",
            "--visualization-type",
            "bucket",
        ])
        .assert()
        .success();

    // visualize writes `<figure>_raw.csv` + a chart image under figures/.
    let figures = exp_path.join("figures");
    assert!(
        figures.join("smoke_raw.csv").exists(),
        "smoke_raw.csv not produced"
    );
    let count = std::fs::read_dir(&figures)
        .unwrap()
        .filter_map(Result::ok)
        .count();
    assert!(
        count >= 2,
        "figures/ should contain raw csv + at least one chart, got {count}"
    );
}

#[test]
#[serial]
fn bash_generates_steps_script() {
    let fx = TestEtna::new();
    let t1 = fx.plant_workload_repo("T1");

    etna().args(["bash", "--path"]).arg(&t1).assert().success();

    let script = fx.scratch().join("steps.sh");
    let contents = std::fs::read_to_string(&script)
        .unwrap_or_else(|e| panic!("failed to read {}: {}", script.display(), e));
    assert!(
        contents.starts_with("#!/bin/bash"),
        "steps.sh missing shebang; got:\n{}",
        &contents[..contents.len().min(200)]
    );
}
