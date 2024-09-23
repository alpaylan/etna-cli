use std::path::{Path, PathBuf};

use log::debug;
use serde_derive::{Deserialize, Serialize};

use crate::git_driver;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Snapshot {
    pub path: PathBuf,
    pub typ: SnapshotType,
    pub hash: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash)]
pub(crate) enum SnapshotType {
    #[serde(rename = "etna")]
    Etna { branch: String },
    #[serde(rename = "script")]
    Script { name: String },
    #[serde(rename = "workload")]
    Workload { name: String, language: String },
    #[serde(rename = "experiment")]
    Experiment { time: String },
}

impl SnapshotType {
    pub(crate) fn is_experiment(&self) -> bool {
        matches!(self, Self::Experiment { .. })
    }

    pub(crate) fn time(&self) -> i64 {
        match self {
            Self::Experiment { time } => chrono::DateTime::parse_from_rfc3339(time)
                .unwrap()
                .timestamp(),
            _ => i64::MIN,
        }
    }

    pub(crate) fn name(&self) -> anyhow::Result<String> {
        match self {
            Self::Script { name } => Ok(name.clone()),
            Self::Workload { name, .. } => Ok(name.clone()),
            _ => anyhow::bail!("name() is not supported for {:?}", self),
        }
    }
}

impl Snapshot {
    pub(crate) fn head(repo_path: &Path, typ: SnapshotType) -> anyhow::Result<Self> {
        let hash = git_driver::head_hash(repo_path)?;
        Ok(Self {
            path: repo_path.to_path_buf(),
            typ,
            hash,
        })
    }

    pub(crate) fn take(
        repo_path: &Path,
        index_path: &Path,
        typ: SnapshotType,
    ) -> anyhow::Result<Self> {
        let hash = git_driver::hash(repo_path, index_path)?;
        debug!(
            "hash of {} = {}",
            repo_path.join(index_path).display(),
            hash
        );

        Ok(Self {
            path: repo_path.join(index_path),
            typ,
            hash,
        })
    }
}
