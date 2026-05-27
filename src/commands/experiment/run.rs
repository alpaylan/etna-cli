use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use serde_json::Value;
use tracing::info;

use crate::{
    driver::run_experiment,
    error_context::Context,
    experiment::{ExperimentMetadata, Test},
    git_driver,
    manager::Manager,
    service::{
        experiment::list_tests,
        test_utils::{build_invalid_test_message, resolve_test_name},
    },
};

fn get_tests(tests: &Vec<String>, experiment: &ExperimentMetadata) -> anyhow::Result<Vec<Test>> {
    if tests.is_empty() {
        anyhow::bail!("No tests provided. Please specify at least one test to run. Try running `etna experiment list-tests` to see available tests.");
    }

    let available_tests = list_tests(&experiment.path)?
        .into_iter()
        .map(|t| t.name)
        .collect::<Vec<_>>();
    if available_tests.is_empty() {
        anyhow::bail!(
            "No tests found in '{}'. Add workloads first (for example: `etna workload add <lang> <workload>`).",
            experiment.path.join("tests").display()
        );
    }

    let mut all_tests = Vec::new();
    for test in tests {
        let resolved_name = resolve_test_name(&test, &available_tests)
            .with_context(|| build_invalid_test_message(&test, &available_tests))?;
        let test_path = experiment
            .path
            .join("tests")
            .join(resolved_name)
            .with_extension("json");

        let content = std::fs::read_to_string(&test_path)
            .with_context(|| format!("Failed to read test from '{}'", test_path.display()))?;
        let test: Vec<Test> = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse test from '{}'", test_path.display()))?;
        all_tests.extend(test);
    }
    Ok(all_tests)
}

pub fn invoke(
    mut mgr: Manager,
    experiment: ExperimentMetadata,
    test_names: Vec<String>,
    short_circuit: bool,
    parallel: bool,
    cli_params: Vec<(String, String)>,
) -> anyhow::Result<()> {
    tracing::trace!("running experiment with name '{:?}'", experiment.name);
    let mut tests =
        get_tests(&test_names, &experiment).context("Failed to get tests for the experiment")?;

    // Convert CLI params to HashMap
    let cli_params: HashMap<String, String> = cli_params.into_iter().collect();

    // Load metrics from the store
    mgr.set_store_path(experiment.store.clone())?;
    mgr.require_store_mut()?.load_metrics()?;

    git_driver::commit(
        &experiment.path,
        &format!(
            "Running 'etna experiment run --name \"{}\" --tests \"{}\"{}{}{}'",
            experiment.name,
            test_names.join(", "),
            if short_circuit {
                " --short-circuit"
            } else {
                ""
            },
            if parallel { " --parallel" } else { "" },
            if !cli_params.is_empty() {
                format!(
                    " {}",
                    cli_params
                        .iter()
                        .map(|(k, v)| format!("--param {}={}", k, v))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            } else {
                "".to_string()
            }
        ),
    )?;

    let mgr = Arc::new(Mutex::new(mgr));

    for test in &mut tests {
        // `trials` and `timeout` are top-level run-loop fields rather than
        // step-template params, so apply them onto the test directly. This lets
        // `--params trials=1 --params timeout=5` shorten a run (e.g. a smoke
        // test) without editing the test file.
        if let Some(v) = cli_params.get("trials") {
            test.trials = v.parse().with_context(|| {
                format!("--params trials must be a non-negative integer, got '{v}'")
            })?;
        }
        if let Some(v) = cli_params.get("timeout") {
            test.timeout = v.parse().with_context(|| {
                format!("--params timeout must be a number of seconds, got '{v}'")
            })?;
        }
        info!("Running test: {}", test);
        for p in cli_params.iter() {
            test.params
                .as_mut()
                .and_then(|params| params.insert(p.0.clone(), Value::String(p.1.clone())));
        }
        run_experiment(
            mgr.clone(),
            test,
            &experiment,
            short_circuit,
            parallel,
            &cli_params,
            None, // No cancel flag for CLI
        )?;
    }

    git_driver::commit(&experiment.path, "Experiment is completed")?;

    Ok(())
}
