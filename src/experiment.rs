use std::path::PathBuf;

use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Experiment {
    pub name: String,
    pub description: String,
    pub path: PathBuf,
}
