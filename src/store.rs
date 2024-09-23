use std::{collections::HashSet, path::PathBuf};

use anyhow::{Context, Ok};
use serde_derive::{Deserialize, Serialize};

use crate::{
    config::{EtnaConfig, ExperimentConfig},
    experiment::{Experiment, ExperimentSnapshot},
    snapshot::{self, Snapshot, SnapshotType},
    workload::Workload,
};

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Store {
    pub metrics: Vec<Metric>,
    pub snapshots: HashSet<Snapshot>,
    pub experiments: HashSet<Experiment>,
}

impl Store {
    pub(crate) fn default() -> Self {
        Store {
            metrics: Vec::new(),
            snapshots: HashSet::new(),
            experiments: HashSet::new(),
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
        etna_config: &EtnaConfig,
        experiment_config: &ExperimentConfig,
    ) -> anyhow::Result<ExperimentSnapshot> {
        let etna_snapshot = snapshot::Snapshot::head(
            &etna_config.repo_dir,
            SnapshotType::Etna {
                branch: etna_config.branch.clone(),
            },
        )
        .context("Failed to take etna snapshot")?;

        self.snapshots.insert(etna_snapshot.clone());

        let experiment_snapshot = snapshot::Snapshot::take(
            &experiment_config.path,
            &PathBuf::from("*"),
            snapshot::SnapshotType::Experiment {
                time: chrono::Utc::now().to_rfc3339(),
            },
        )
        .context("Failed to take experiment snapshot")?;

        self.snapshots.insert(experiment_snapshot.clone());

        let collection_script_snapshot = snapshot::Snapshot::take(
            &experiment_config.path,
            &PathBuf::from("Collect.py"),
            snapshot::SnapshotType::Script {
                name: "Collect.py".to_string(),
            },
        )
        .context("Failed to take Collect.py snapshot")?;

        self.snapshots.insert(collection_script_snapshot.clone());

        let workload_snapshots: Vec<(Workload, String)> = experiment_config
            .workloads
            .iter()
            .map(|workload| {
                let workload_snapshot = snapshot::Snapshot::take(
                    &experiment_config.path,
                    &PathBuf::from("workloads")
                        .join(PathBuf::from(&workload.language))
                        .join(PathBuf::from(&workload.name))
                        .join("*"),
                    snapshot::SnapshotType::Workload {
                        name: workload.name.clone(),
                        language: workload.language.clone(),
                    },
                )
                .context("Failed to take workloads snapshot")?;
                self.snapshots.insert(workload_snapshot.clone());

                Ok((workload.clone(), workload_snapshot.hash))
            })
            .filter_map(Result::ok)
            .collect();

        Ok(ExperimentSnapshot {
            experiment: experiment_snapshot.hash,
            etna: etna_snapshot.hash,
            scripts: vec![("Collect.py".to_string(), collection_script_snapshot.hash)],
            workloads: workload_snapshots,
        })
    }
}

impl Store {
    pub(crate) fn get_experiment_by_name(&self, name: &str) -> anyhow::Result<&Experiment> {
        let experiments = self
            .experiments
            .iter()
            .filter(|experiment| experiment.name == name)
            .collect::<Vec<&Experiment>>();

        let experiment_hashes = experiments
            .iter()
            .map(|experiment| experiment.id.clone())
            .collect::<Vec<String>>();

        let snapshots = self
            .snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.typ.is_experiment() && experiment_hashes.contains(&snapshot.hash)
            })
            .collect::<Vec<&Snapshot>>();

        let latest_snapshot = snapshots
            .iter()
            .max_by(|a, b| a.typ.time().cmp(&b.typ.time()))
            .context("No snapshots found")?;

        let latest_experiment = self
            .experiments
            .iter()
            .find(|experiment| experiment.id == latest_snapshot.hash)
            .context("No experiment found")?;

        Ok(latest_experiment)
    }

    pub(crate) fn get_all_experiments_by_name(&self, name: &str) -> Vec<&Experiment> {
        self.experiments
            .iter()
            .filter(|experiment| experiment.name == name)
            .collect::<Vec<&Experiment>>()
    }

    pub(crate) fn get_experiment_by_id(&self, hash: &str) -> anyhow::Result<&Experiment> {
        self.experiments
            .iter()
            .find(|experiment| experiment.id == hash)
            .context("Experiment not found")
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Metric {
    pub data: serde_json::Value,
    pub experiment_id: String,
}

pub(crate) trait Queriable {
    fn query(&self, store: &Store) -> anyhow::Result<Vec<String>>;
}

pub(crate) enum SpecializedQuery {
    Experiment(ExperimentQuery),
    Metric(MetricQuery),
    Snapshot(SnapshotQuery),
}

impl Queriable for SpecializedQuery {
    fn query(&self, store: &Store) -> anyhow::Result<Vec<String>> {
        match self {
            SpecializedQuery::Experiment(query) => query.query(store),
            SpecializedQuery::Metric(query) => query.query(store),
            SpecializedQuery::Snapshot(query) => query.query(store),
        }
    }
}

pub(crate) enum ExperimentQuery {
    Id(String),
    NameLast(String),
    NameAll(String),
}

impl Queriable for ExperimentQuery {
    fn query(&self, store: &Store) -> anyhow::Result<Vec<String>> {
        match self {
            ExperimentQuery::Id(hash) => {
                let experiment = store.get_experiment_by_id(hash)?;
                Ok(vec![serde_json::to_string(experiment)?])
            }
            ExperimentQuery::NameLast(name) => {
                let experiment = store.get_experiment_by_name(name)?;
                Ok(vec![serde_json::to_string(experiment)?])
            }
            ExperimentQuery::NameAll(name) => {
                let experiments = store.get_all_experiments_by_name(name);
                experiments
                    .iter()
                    .map(|e| serde_json::to_string(e).context("Failed to serialize experiment"))
                    .collect()
            }
        }
    }
}

pub(crate) enum MetricQuery {
    ByExperimentId(String),
}

impl Queriable for MetricQuery {
    fn query(&self, store: &Store) -> anyhow::Result<Vec<String>> {
        match self {
            MetricQuery::ByExperimentId(hash) => {
                let metrics = store
                    .metrics
                    .iter()
                    .filter(|metric| metric.experiment_id == *hash)
                    .collect::<Vec<&Metric>>();

                metrics
                    .iter()
                    .map(|m| serde_json::to_string(m).context("Failed to serialize metric"))
                    .collect()
            }
        }
    }
}

pub(crate) enum SnapshotQuery {
    ByName(String),
    ByHash(String),
}

impl Queriable for SnapshotQuery {
    fn query(&self, store: &Store) -> anyhow::Result<Vec<String>> {
        match self {
            SnapshotQuery::ByName(name) => {
                let snapshots = store
                    .snapshots
                    .iter()
                    .filter(|snapshot| snapshot.typ.name().unwrap_or("".to_string()) == *name)
                    .collect::<Vec<&Snapshot>>();

                snapshots
                    .iter()
                    .map(|s| serde_json::to_string(s).context("Failed to serialize snapshot"))
                    .collect()
            }
            SnapshotQuery::ByHash(hash) => {
                let snapshot = store
                    .snapshots
                    .iter()
                    .find(|snapshot| snapshot.hash == *hash)
                    .context("Snapshot not found")?;

                Ok(vec![
                    serde_json::to_string(snapshot).context("Failed to serialize snapshot")?
                ])
            }
        }
    }
}
