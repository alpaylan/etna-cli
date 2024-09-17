use std::fs;

use anyhow::Context;

use crate::{config::ExperimentConfig, git_driver};

pub(crate) fn invoke(language: String, workload: String) -> anyhow::Result<()> {
    // Get the current directory
    let current_dir = std::env::current_dir().context("Failed to get current directory")?;
    // Check for the config file
    let config_path = current_dir.join("config.toml");
    if !config_path.exists() {
        anyhow::bail!("No experiment found in '{}'", current_dir.display());
    }

    // Read the config file
    let config = std::fs::read_to_string(&config_path).context("Failed to read config file")?;
    let config: ExperimentConfig =
        toml::from_str(&config).context("Failed to parse config file")?;

    // Check if the workload already exists
    if !config.has_workload(&language, &workload) {
        anyhow::bail!("Workload '{}/{}' does not exist", language, workload);
    }

    // Remove the workload from the config
    let mut config = config;
    config
        .workloads
        .retain(|w| w.language != language || w.name != workload);

    // Write the updated config file
    std::fs::write(
        &config_path,
        toml::to_string(&config).context("Failed to serialize configuration")?,
    )
    .context("Failed to write config file")?;

    // Remove the workload from the experiment directory
    let dest_path = current_dir
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
