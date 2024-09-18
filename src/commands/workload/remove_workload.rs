use std::fs;

use anyhow::Context;

use crate::{
    config::{EtnaConfig, ExperimentConfig},
    git_driver,
};

pub(crate) fn invoke(
    name: Option<String>,
    language: String,
    workload: String,
) -> anyhow::Result<()> {
    // Get etna configuration
    let etna_config = EtnaConfig::get_etna_config().context("Failed to get etna config")?;
    // Get the current experiment
    let mut experiment_config = name
        .context("No experiment name provided")
        .and_then(|n| ExperimentConfig::from_etna_config(&n, &etna_config))
        .or_else(|_| ExperimentConfig::from_current_dir())?;

    // Check if the workload already exists
    if !experiment_config.has_workload(&language, &workload) {
        anyhow::bail!("Workload '{}/{}' does not exist", language, workload);
    }

    // Remove the workload from the config
    experiment_config
        .workloads
        .retain(|w| w.language != language || w.name != workload);

    // Write the updated config file
    let config_path = experiment_config.path.join("config.toml");
    std::fs::write(
        &config_path,
        toml::to_string(&experiment_config).context("Failed to serialize configuration")?,
    )
    .context("Failed to write config file")?;

    // Remove the workload from the experiment directory
    let dest_path = experiment_config
        .path
        .join("workloads")
        .join(&language)
        .join(&workload);

    fs::remove_dir_all(&dest_path).context(format!(
        "Failed to remove workload at '{}'",
        dest_path.display()
    ))?;

    // Create a commit
    git_driver::commit_remove_workload(&language, &workload)
        .with_context(|| format!("Failed to commit removing '{language}/{workload}'"))?;

    Ok(())
}
