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

/// Deserialized form of `etna.toml` at an experiment repo root. Consumed at
/// clone time; written at scaffold time by `create_experiment`.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ExperimentManifest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

impl ExperimentManifest {
    pub fn read(dir: &std::path::Path) -> anyhow::Result<Self> {
        let manifest_path = dir.join("etna.toml");
        let body = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read '{}'", manifest_path.display()))?;
        toml::from_str::<Self>(&body)
            .with_context(|| format!("Failed to parse '{}'", manifest_path.display()))
    }
}

impl ExperimentMetadata {
    /// Resolve a workload name to its on-disk path by walking `workloads/`
    /// recursively. A directory that contains `steps.json` is a workload and
    /// terminates descent; other directories are treated as organizational
    /// folders and recursed into.
    pub(crate) fn workload_path(&self, name: &str) -> Option<std::path::PathBuf> {
        let root = self.path.join("workloads");
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
                {
                    continue;
                }
                if path.join("steps.json").exists() {
                    if path.file_name().and_then(|n| n.to_str()) == Some(name) {
                        return Some(path);
                    }
                    // don't descend into a workload
                } else if path.join(".git").exists() {
                    // git repo without steps.json — partially-set-up workload; stop here
                } else {
                    stack.push(path);
                }
            }
        }
        None
    }

    pub(crate) fn workloads(&self) -> Vec<WorkloadMetadata> {
        let root = self.path.join("workloads");
        let mut workloads = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                if path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with('.'))
                {
                    continue;
                }
                if path.join("steps.json").exists() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        workloads.push(WorkloadMetadata {
                            name: name.to_string(),
                        });
                    }
                } else if path.join(".git").exists() {
                    // git repo without steps.json — partially-set-up workload; stop here
                } else {
                    stack.push(path);
                }
            }
        }

        workloads.sort_by(|a, b| a.name.cmp(&b.name));
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

/// The workload to draw a capability from (for Cross mode producer/consumer).
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Target {
    pub workload: String,
}

/// The experiment mode — selects which capability pipeline to run.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Default)]
pub enum Mode {
    /// Run the full PBT campaign in the workload's framework.
    #[default]
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
    pub workload: String,
    pub trials: usize,
    pub timeout: f64,
    pub mutations: Vec<String>,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub params: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub tasks: Vec<HashMap<String, serde_json::Value>>,
}

impl Display for Test {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "(workload: {}, trials: {}, timeout: {}, mode: {}, mutations: {:?}, tasks: {:?})",
            self.workload,
            self.trials,
            self.timeout,
            self.mode.name(),
            self.mutations,
            self.tasks
        )
    }
}
