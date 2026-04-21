use std::path::PathBuf;

use serde_derive::{Deserialize, Serialize};

use crate::error_context::Context;

/// Etna Configuration
/// It contains the configuration for etna-cli
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct EtnaConfig {
    pub etna_dir: PathBuf,
    pub configured: bool,
    #[serde(default = "default_version")]
    pub version: usize,
}

fn default_version() -> usize {
    1
}

pub(crate) fn current_version() -> usize {
    2
}

impl EtnaConfig {
    //! ETNA v2 notes:
    //! - Removed experiment configurations
    //! - Added explicit versioning to the configuration for future changes
    //! - Experiment metrics are stored in each experiment's local `store.jsonl`
    //! - Experiments are now not part of the store, but managed separately in `experiments.json`
    //! - Switched to using JSON lines format for consuming metrics and logs instead of using explicitly marked JSONs.
    pub(crate) fn new() -> anyhow::Result<Self> {
        let etna_dir = Self::get_etna_dir()?;
        let configured = false;
        let version = current_version();

        Ok(Self {
            etna_dir,
            configured,
            version,
        })
    }

    pub(crate) fn get_etna_dir() -> anyhow::Result<PathBuf> {
        if let Some(override_dir) = std::env::var_os("ETNA_HOME") {
            if !override_dir.is_empty() {
                return Ok(PathBuf::from(override_dir));
            }
        }

        dirs::home_dir()
            .map(|home_dir| home_dir.join(".etna"))
            .ok_or_else(|| anyhow::anyhow!("Failed to get home directory"))
    }

    pub(crate) fn get_etna_config() -> anyhow::Result<Self> {
        tracing::trace!("loading etna configuration");
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
        tracing::trace!("saving etna configuration");
        let config_path = self.etna_dir.join("config.json");
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
    pub(crate) fn store_path(&self) -> PathBuf {
        self.etna_dir.join("store.jsonl")
    }

    pub(crate) fn experiments_path(&self) -> PathBuf {
        self.etna_dir.join("experiments.json")
    }
}
