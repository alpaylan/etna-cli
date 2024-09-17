use std::{fs, path::PathBuf, process::Command};

use anyhow::Context;

use crate::{
    config::{EtnaConfig, ExperimentConfig},
    git_driver,
    workload::Workload,
};

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
    if config.has_workload(&language, &workload) {
        anyhow::bail!("Workload '{}/{}' already exists", language, workload);
    }

    let etna_config = EtnaConfig::get_etna_config().context("Failed to get etna config")?;
    // get etna directory
    let repo_dir = if let Ok(repo_dir) =
        std::env::var("ETNA_REPO_DIR").context("ETNA_REPO_DIR environment variable not set")
    {
        PathBuf::from(repo_dir)
    } else {
        etna_config.repo_dir
    };

    // Get the workload path
    let workload_path = repo_dir.join("workloads").join(&language).join(&workload);

    // Check if the workload exists
    if !workload_path.exists() {
        anyhow::bail!("Workload '{}' not found", workload_path.display());
    }

    // Copy the workload to the experiment directory
    let experiment_path = current_dir;
    let dest_path = experiment_path
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
    let mut config = config;
    config.workloads.push(Workload {
        language: language.clone(),
        name: workload.clone(),
    });

    // Write the updated config file
    std::fs::write(
        &config_path,
        toml::to_string(&config).context("Failed to serialize configuration")?,
    )
    .context("Failed to write config file")?;

    // Create a commit
    git_driver::commit_add_workload(&language, &workload)
        .with_context(|| format!("Failed to commit adding '{language}/{workload}'"))?;

    Ok(())
}
