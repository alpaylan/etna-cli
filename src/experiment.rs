use std::{collections::HashMap, fmt::Display, path::PathBuf};

use serde_derive::{Deserialize, Serialize};

use crate::{git_driver, manager::Manager, workload::WorkloadMetadata};
use crate::error_context::Context as _;

/// Experiment Configuration
/// It contains the name of the experiment, a description of the experiment, and a list of workloads
/// to be executed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentMetadata {
    pub name: String,
    pub path: PathBuf,
    pub store: PathBuf,
}

impl ExperimentMetadata {
    pub(crate) fn has_workload(&self, language: &str, name: &str) -> bool {
        self.path
            .join("workloads")
            .join(language)
            .join(name)
            .exists()
    }
    pub(crate) fn workloads(&self) -> Vec<WorkloadMetadata> {
        let workloads_path = self.path.join("workloads");
        let mut workloads = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&workloads_path) {
            for entry in entries.flatten() {
                let lang_path = entry.path();
                if lang_path.is_dir() {
                    if let Ok(lang_entries) = std::fs::read_dir(&lang_path) {
                        for lang_entry in lang_entries.flatten() {
                            let workload_path = lang_entry.path();
                            if workload_path.is_dir() {
                                // It is onl;y a workload if it has a `steps.json` file
                                if !workload_path.join("steps.json").exists() {
                                    continue;
                                }
                                if let Some(workload_name) =
                                    workload_path.file_name().and_then(|n| n.to_str())
                                {
                                    if let Some(language_name) =
                                        lang_path.file_name().and_then(|n| n.to_str())
                                    {
                                        workloads.push(WorkloadMetadata {
                                            name: workload_name.to_string(),
                                            language: language_name.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        workloads
    }

    pub(crate) fn hash(&self) -> anyhow::Result<String> {
        // get the git head at the top
        git_driver::_head_hash(&self.path)
    }
}

impl ExperimentMetadata {
    pub fn from_current_dir(mgr: &Manager) -> anyhow::Result<Self> {
        // Find an experiment in the manager's list that is a parent of the current directory
        let current_dir = std::env::current_dir().context("Failed to get current directory")?;

        let experiment = mgr
            .experiments
            .values()
            .find(|exp| current_dir.starts_with(&exp.path))
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Current directory is not inside any known experiment. Current dir: {}",
                    current_dir.display()
                )
            })?;

        Ok(experiment)
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Test {
    pub language: String,
    pub workload: String,
    pub trials: usize,
    pub timeout: f64,
    pub mutations: Vec<String>,
    #[serde(default)]
    pub cross: bool,
    #[serde(default)]
    pub params: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub tasks: Vec<HashMap<String, String>>,
}

impl Display for Test {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "(language: {}, workload: {}, trials: {}, timeout: {}, cross: {}, mutations: {:?}, tasks: {:?})",
            self.language, self.workload, self.trials, self.timeout, self.cross, self.mutations, self.tasks
        )
    }
}
