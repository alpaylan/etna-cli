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

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum SnapshotType {
    #[serde(rename = "etna")]
    Etna,
    #[serde(rename = "collection_script")]
    CollectionScript,
    #[serde(rename = "workload")]
    Workload,
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
