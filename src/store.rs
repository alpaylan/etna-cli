use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Context, Ok};
use serde_derive::{Deserialize, Serialize};

use crate::{
    experiment::{Experiment, ExperimentSnapshot},
    snapshot::{self, Snapshot},
};

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Store {
    pub metrics: Vec<Metric>,
    pub snapshots: HashSet<Snapshot>,
    pub experiments: Vec<Experiment>,
}

impl Store {
    pub(crate) fn default() -> Self {
        Store {
            metrics: Vec::new(),
            snapshots: HashSet::new(),
            experiments: Vec::new(),
        }
    }

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

    pub(crate) fn take_snapshot(
        &mut self,
        etna_repo_dir: &Path,
        experiment_path: &Path,
    ) -> anyhow::Result<ExperimentSnapshot> {
        let etna_snapshot = snapshot::Snapshot::head(etna_repo_dir, snapshot::SnapshotType::Etna)
            .context("Failed to take etna snapshot")?;

        self.snapshots.insert(etna_snapshot.clone());

        let collection_script_snapshot = snapshot::Snapshot::take(
            experiment_path,
            &PathBuf::from("Collect.py"),
            snapshot::SnapshotType::CollectionScript,
        )
        .context("Failed to take Collect.py snapshot")?;

        self.snapshots.insert(collection_script_snapshot.clone());

        let workload_snapshot = snapshot::Snapshot::take(
            experiment_path,
            &PathBuf::from("workloads").join("*"),
            snapshot::SnapshotType::Workload,
        )
        .context("Failed to take workloads snapshot")?;

        self.snapshots.insert(workload_snapshot.clone());

        Ok(ExperimentSnapshot {
            etna: etna_snapshot.hash,
            collection_script: collection_script_snapshot.hash,
            workload: workload_snapshot.hash,
        })
    }
}

impl Store {
    pub(crate) fn get_experiment(&self, name: &str) -> anyhow::Result<&Experiment> {
        self.experiments
            .iter()
            .find(|experiment| experiment.name == name)
            .context("Experiment not found")
    }

    pub(crate) fn get_experiment_mut(&mut self, name: &str) -> anyhow::Result<&mut Experiment> {
        self.experiments
            .iter_mut()
            .find(|experiment| experiment.name == name)
            .context("Experiment not found")
    }

    pub(crate) fn update_snapshot(
        &mut self,
        experiment_name: &str,
        snapshot: ExperimentSnapshot,
    ) -> anyhow::Result<()> {
        let experiment = self.get_experiment_mut(experiment_name)?;

        experiment.snapshot = snapshot;

        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Metric {
    pub data: serde_json::Value,
    pub experiment_id: String,
    pub snapshot_id: String,
}
