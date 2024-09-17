use std::path::PathBuf;

use crate::workload::Workload;
use anyhow::Context;
use serde_derive::{Deserialize, Serialize};

/// Experiment Configuration
/// It contains the name of the experiment, a description of the experiment, and a list of workloads
/// to be executed.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ExperimentConfig {
    pub name: String,
    pub description: String,
    pub workloads: Vec<Workload>,
}

impl ExperimentConfig {
    pub(crate) fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            workloads: vec![],
        }
    }
}

impl ExperimentConfig {
    pub(crate) fn has_workload(&self, language: &str, name: &str) -> bool {
        self.workloads
            .iter()
            .any(|w| w.language == language && w.name == name)
    }
}

/// Etna Configuration
/// It contains the configuration for etna-cli
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct EtnaConfig {
    pub etna_dir: PathBuf,
    pub repo_dir: PathBuf,
    pub venv_dir: PathBuf,
    pub branch: String,
    pub configured: bool,
}

impl EtnaConfig {
    pub(crate) fn new(branch: String) -> anyhow::Result<Self> {
        let etna_dir = Self::get_etna_dir()?;

        let repo_dir = etna_dir.join("etna");
        let venv_dir = etna_dir.join(".venv");
        let configured = false;

        Ok(Self {
            etna_dir,
            repo_dir,
            venv_dir,
            branch,
            configured,
        })
    }

    pub(crate) fn get_etna_dir() -> anyhow::Result<PathBuf> {
        dirs::home_dir()
            .map(|home_dir| home_dir.join(".etna"))
            .ok_or_else(|| anyhow::anyhow!("Failed to get home directory"))
    }

    pub(crate) fn get_etna_config() -> anyhow::Result<Self> {
        let config_path = Self::get_etna_dir()?.join("config.json");
        if let Ok(file) = std::fs::File::open(&config_path) {
            serde_json::from_reader(file).context("Failed to read config.json")
        } else {
            Err(anyhow::anyhow!("Failed to read config.json"))
        }
    }

    pub(crate) fn save(&self) -> anyhow::Result<()> {
        let config_path = self.config_path();
        let file = std::fs::File::create(&config_path)
            .context("Failed to create config file")?;
        serde_json::to_writer_pretty(file, self).context("Failed to write config.json")
    }
}

impl EtnaConfig {
    pub(crate) fn config_path(&self) -> PathBuf {
        self.etna_dir.join("config.json")
    }

    pub(crate) fn store_path(&self) -> PathBuf {
        self.etna_dir.join("store.json")
    }

}
