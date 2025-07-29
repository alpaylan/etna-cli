use std::path::PathBuf;

use crate::{store::Store, workload::WorkloadMetadata};
use anyhow::Context;
use serde_derive::{Deserialize, Serialize};

/// Experiment Configuration
/// It contains the name of the experiment, a description of the experiment, and a list of workloads
/// to be executed.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExperimentConfig {
    pub name: String,
    pub description: String,
    pub workloads: Vec<WorkloadMetadata>,
    #[serde(skip)]
    #[serde(default)]
    pub path: PathBuf,
    pub store: PathBuf,
}

impl ExperimentConfig {
    pub fn new(name: String, description: String, path: PathBuf, store: PathBuf) -> Self {
        Self {
            name,
            description,
            workloads: vec![],
            path,
            store,
        }
    }

    pub(crate) fn from_path(path: PathBuf) -> anyhow::Result<Self> {
        // Check for the config file
        let config_path = path.join("config.toml");
        if !config_path.exists() {
            anyhow::bail!("No experiment found in '{}'", path.display());
        }

        // Read the config file
        let config = std::fs::read_to_string(&config_path).context("Failed to read config file")?;
        let mut config: ExperimentConfig =
            toml::from_str(&config).context("Failed to parse config file")?;
        config.path = path;

        Ok(config)
    }

    pub(crate) fn from_current_dir() -> anyhow::Result<Self> {
        log::trace!("Attempting to load experiment config from current directory");
        Self::from_path(std::env::current_dir().context("Failed to get current directory")?)
    }

    pub(crate) fn from_etna_config(name: &str, etna_config: &EtnaConfig) -> anyhow::Result<Self> {
        let store = Store::load(&etna_config.store_path())?;
        let experiment = store
            .experiments
            .iter()
            .find(|e| e.name == name)
            .context("Failed to find experiment")?;

        Self::from_path(experiment.path.clone())
    }

    pub(crate) fn from_name(name: &str) -> anyhow::Result<Self> {
        let etna_config = EtnaConfig::get_etna_config()?;
        Self::from_etna_config(name, &etna_config)
    }

    pub(crate) fn from_maybe_name(name: Option<&str>) -> anyhow::Result<Self> {
        match name {
            Some(name) => Self::from_name(name),
            None => Self::from_current_dir(),
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
    pub configured: bool,
}

impl EtnaConfig {
    pub(crate) fn new() -> anyhow::Result<Self> {
        let etna_dir = Self::get_etna_dir()?;
        let configured = false;

        Ok(Self {
            etna_dir,
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
            anyhow::bail!(format!(
                "Failed to read configuration at '{}'",
                config_path.display()
            ))
        }
    }

    pub(crate) fn _save(&self) -> anyhow::Result<()> {
        let config_path = self._config_path();
        let file = std::fs::File::create(&config_path).with_context(|| {
            format!(
                "Failed to create configuration file at '{}'",
                config_path.display()
            )
        })?;
        serde_json::to_writer_pretty(file, self).with_context(|| {
            format!(
                "Failed to write to the configuration file at '{}'",
                config_path.display()
            )
        })
    }
}

impl EtnaConfig {
    pub(crate) fn _config_path(&self) -> PathBuf {
        self.etna_dir.join("config.json")
    }

    pub(crate) fn store_path(&self) -> PathBuf {
        self.etna_dir.join("store.json")
    }
}
