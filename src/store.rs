use std::path::PathBuf;

use anyhow::Context;
use serde_derive::{Deserialize, Serialize};

use crate::experiment::Experiment;

#[derive(Debug, Default, Serialize, Deserialize)]

pub(crate) struct Store {
    pub metrics: Vec<Metric>,
    pub snapshots: Vec<Snapshot>,
    pub experiments: Vec<Experiment>,
}

impl Store {
    pub(crate) fn load(path: &PathBuf) -> anyhow::Result<Self> {
        if !path.exists() {
            anyhow::bail!("Store file does not exist");
        }

        let content = std::fs::read_to_string(path)?;
        let store: Store = serde_json::from_str(&content)?;

        Ok(store)
    }

    pub(crate) fn save(&self, path: &PathBuf) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;

        std::fs::write(path, content).context("Failed to write store file")
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Metric {
    pub data: serde_json::Value,
    pub experiment_id: String,
    pub snapshot_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    pub etna: String,
    pub collection_script: String,
    pub workload: String,
}
