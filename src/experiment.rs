use std::path::PathBuf;

use serde_derive::{Deserialize, Serialize};

use crate::workload::Workload;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub(crate) struct Experiment {
    pub name: String,
    pub id: String,
    pub description: String,
    pub path: PathBuf,
    pub snapshot: ExperimentSnapshot,
}

impl Experiment {
    pub(crate) fn with_snapshot(&self, snapshot: ExperimentSnapshot) -> Self {
        Self {
            id: snapshot.experiment.clone(),
            snapshot,
            ..self.clone()
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub(crate) struct ExperimentSnapshot {
    pub experiment: String,
    pub etna: String,
    pub scripts: Vec<(String, String)>,
    pub workloads: Vec<(Workload, String)>,
}
