use std::{fs, path::PathBuf, process::Command};

use anyhow::Context;

use crate::{
    config::{EtnaConfig, ExperimentConfig},
    experiment, git_driver, store,
    workload::Workload,
};

pub(crate) fn invoke(
    experiment_name: Option<String>,
    language: String,
    workload: String,
) -> anyhow::Result<()> {
    // Get etna configuration
    let etna_config = EtnaConfig::get_etna_config().context("Failed to get etna config")?;
    // Get the current experiment
    let mut experiment_config = experiment_name
        .ok_or(anyhow::anyhow!("No experiment name provided"))
        .and_then(|n| ExperimentConfig::from_etna_config(&n, &etna_config))
        .or_else(|_| ExperimentConfig::from_current_dir())
        .context("No experiment name is provided, and the current directory is not an experiment directory")?;

    // Check if the workload already exists
    if experiment_config.has_workload(&language, &workload) {
        anyhow::bail!("Workload '{}/{}' already exists", language, workload);
    }

    // get etna directory
    let repo_dir = if let Ok(repo_dir) =
        std::env::var("ETNA_REPO_DIR").context("ETNA_REPO_DIR environment variable not set")
    {
        PathBuf::from(repo_dir)
    } else {
        etna_config.repo_dir.clone()
    };

    // Get the workload path
    let workload_path = repo_dir.join("workloads").join(&language).join(&workload);

    // Check if the workload exists
    if !workload_path.exists() {
        anyhow::bail!("Workload '{}' not found", workload_path.display());
    }

    // Copy the workload to the experiment directory
    let dest_path = experiment_config
        .path
        .join("workloads")
        .join(&language)
        .join(&workload);

    std::fs::create_dir_all(
        dest_path
            .parent()
            .context("Failed to get parent directory")?,
    )
    .context("Failed to create parent directory")?;

    Command::new("cp")
        .arg("-r")
        .arg(&workload_path)
        .arg(&dest_path)
        .status()
        .context(format!(
            "Failed to copy workload at '{}' to '{}'",
            fs::canonicalize(workload_path)
                .context("Failed to get canonical path")?
                .display(),
            dest_path.display()
        ))?;

    // Add the workload to the config
    experiment_config.workloads.push(Workload {
        language: language.clone(),
        name: workload.clone(),
    });

    // Write the updated config file
    let config_path = experiment_config.path.join("config.toml");
    std::fs::write(
        &config_path,
        toml::to_string(&experiment_config).context("Failed to serialize configuration")?,
    )
    .context("Failed to write config file")?;

    // Create a commit
    git_driver::commit_add_workload(&language, &workload)
        .with_context(|| format!("Failed to commit adding '{language}/{workload}'"))?;

    // Add the snapshot to the store
    let mut store =
        store::Store::load(&etna_config.store_path()).context("Failed to load store")?;

    let snapshot = store.take_snapshot(&etna_config, &experiment_config)?;

    store.experiments.insert(experiment::Experiment {
        name: experiment_config.name,
        id: snapshot.experiment.clone(),
        description: experiment_config.description,
        path: experiment_config.path,
        snapshot,
    });

    store
        .save(&etna_config.store_path())
        .context("Failed to save store")?;

    Ok(())
}
