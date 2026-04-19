use std::{collections::HashMap, fmt::Display, path::PathBuf};

use serde_derive::{Deserialize, Serialize};

use crate::error_context::Context as _;
use crate::{git_driver, manager::Manager, workload::WorkloadMetadata};

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

/// Where the input list comes from for a `Test` mode invocation.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum InputSource {
    /// Read inputs from a file on disk.
    File(std::path::PathBuf),
    /// Inline list of opaque input strings.
    Inline(Vec<String>),
}

/// Where the counterexample comes from for a `Shrink` mode invocation.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum CexSource {
    /// Inline opaque counterexample string.
    Inline(String),
    /// Read counterexample from a file on disk.
    File(std::path::PathBuf),
    /// Pull `counterexample` field from the current task.
    FromTask,
}

/// What `sample` should collect.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy, Default)]
#[serde(rename_all = "snake_case")]
pub enum SampleCollect {
    /// Collect inputs only.
    InputsOnly,
    /// Collect inputs plus per-input metadata (time, stats).
    #[default]
    InputsAndStats,
}

/// A (language, workload) pair identifying the workload to draw a capability from.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Target {
    pub language: String,
    pub workload: String,
}

/// The experiment mode — selects which capability pipeline to run.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum Mode {
    /// Run the full PBT campaign in the workload's framework.
    Solve,
    /// Produce inputs (with optional per-input metadata).
    Sample {
        #[serde(default)]
        collect: SampleCollect,
    },
    /// Consume a list of inputs and run the property over them.
    Test { inputs: InputSource },
    /// Take a single failing input and shrink it.
    Shrink { counterexample: CexSource },
    /// Cross-framework: producer's `sample` capability feeds consumer's `test` capability.
    Cross { producer: Target, consumer: Target },
}

impl Mode {
    /// Short discriminant string used as the `mode` key in metric records.
    pub fn name(&self) -> &'static str {
        match self {
            Mode::Solve => "solve",
            Mode::Sample { .. } => "sample",
            Mode::Test { .. } => "test",
            Mode::Shrink { .. } => "shrink",
            Mode::Cross { .. } => "cross",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Test {
    /// Required for non-Cross modes; ignored for Cross (which carries its own targets).
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub workload: String,
    pub trials: usize,
    pub timeout: f64,
    pub mutations: Vec<String>,
    pub mode: Mode,
    #[serde(default)]
    pub params: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub tasks: Vec<HashMap<String, String>>,
}

impl Display for Test {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "(language: {}, workload: {}, trials: {}, timeout: {}, mode: {}, mutations: {:?}, tasks: {:?})",
            self.language, self.workload, self.trials, self.timeout, self.mode.name(), self.mutations, self.tasks
        )
    }
}
