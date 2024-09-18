use std::path::PathBuf;

use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Experiment {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
    pub snapshot: ExperimentSnapshot,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub(crate) struct ExperimentSnapshot {
    pub etna: String,
    pub collection_script: String,
    pub workload: String,
}
