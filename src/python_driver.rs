use anyhow::{Context, Ok};
use log::{debug, info};

use crate::{
    config::{EtnaConfig, ExperimentConfig},
    experiment::ExperimentSnapshot,
};

pub(crate) fn run_experiment(
    etna_config: &EtnaConfig,
    _experiment_config: &ExperimentConfig,
    snapshot: ExperimentSnapshot,
) -> anyhow::Result<()> {
    std::env::set_var("ETNA_EXPERIMENT_ID", snapshot.experiment);
    debug!(
        "ETNA_EXPERIMENT_ID={:?}",
        std::env::var("ETNA_EXPERIMENT_ID")
    );

    std::env::set_var("VIRTUAL_ENV", etna_config.venv_dir.display().to_string());
    debug!("VIRTUAL_ENV={:?}", std::env::var("VIRTUAL_ENV"));

    std::env::set_var(
        "PATH",
        format!(
            "{}/bin:{}",
            std::env::var("VIRTUAL_ENV")
                .context("VIRTUAL_ENV is not present in the environment")?,
            std::env::var("PATH").context("PATH is not present in the environment")?
        ),
    );
    debug!("PATH={:?}", std::env::var("PATH"));

    std::process::Command::new("python3")
        .args(["Collect.py"])
        .status()
        .context("Failed to run the experiment")?;

    Ok(())
}

pub(crate) fn make(etna_config: &EtnaConfig) -> anyhow::Result<()> {
    // Create a venv for the etna repository
    let etna_venv_dir = etna_config.etna_dir.join(".venv");

    if !etna_venv_dir.exists() {
        info!("Setting up a virtual environment for etna...");
        std::process::Command::new("python3")
            .arg("-m")
            .arg("venv")
            .arg(&etna_venv_dir)
            .status()
            .context("Failed to create virtual environment")?;
    }

    std::env::set_var("VIRTUAL_ENV", &etna_venv_dir);
    debug!("VIRTUAL_ENV={:?}", std::env::var("VIRTUAL_ENV"));

    std::env::set_var(
        "PATH",
        format!(
            "{}/bin:{}",
            std::env::var("VIRTUAL_ENV")
                .context("VIRTUAL_ENV is not present in the environment")?,
            std::env::var("PATH").context("PATH is not present in the environment")?
        ),
    );
    debug!("PATH={:?}", std::env::var("PATH"));

    debug!("make -C {} install", etna_config.repo_dir.display());

    let output = std::process::Command::new("make")
        .args(["-C", &etna_config.repo_dir.display().to_string(), "install"])
        .output()
        .context(format!(
            "Failed to run ETNA setup script at {}",
            etna_config.repo_dir.display()
        ))?;

    debug!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    debug!("stderr: {}", String::from_utf8_lossy(&output.stderr));

    Ok(())
}
