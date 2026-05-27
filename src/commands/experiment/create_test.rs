use crate::{
    experiment::{ExperimentMetadata, Mode},
    git_driver,
    manager::Manager,
    service,
};

use crate::error_context::Context;

pub fn invoke(
    mgr: Manager,
    experiment: ExperimentMetadata,
    workload: String,
    test_name: Option<String>,
    trials: usize,
    timeout: f64,
    mode: String,
    mutations: Vec<String>,
) -> anyhow::Result<()> {
    let test_name = test_name.unwrap_or_else(|| workload.to_lowercase());

    let mode = parse_mode(&mode)?;

    service::experiment::create_test(
        &mgr,
        &experiment,
        &test_name,
        &workload,
        trials,
        timeout,
        mode,
        mutations,
    )?;

    git_driver::commit(&experiment.path, &format!("create test '{}'", test_name))
        .with_context(|| format!("Failed to commit new test '{}'", test_name))?;

    tracing::info!(
        "Created test '{}' for workload '{}' in experiment '{}'",
        test_name,
        workload,
        experiment.name
    );

    Ok(())
}

fn parse_mode(s: &str) -> anyhow::Result<Mode> {
    match s.to_lowercase().as_str() {
        "solve" => Ok(Mode::Solve),
        "sample" => Ok(Mode::Sample {
            collect: Default::default(),
        }),
        "cross" => anyhow::bail!(
            "Mode 'cross' requires producer/consumer targets; edit the generated test JSON to add them."
        ),
        "test" | "shrink" => anyhow::bail!(
            "Mode '{}' requires an input/counterexample source; edit the generated test JSON to add it.",
            s
        ),
        other => anyhow::bail!(
            "Unknown mode '{}'. Valid modes: solve, sample, test, shrink, cross.",
            other
        ),
    }
}
