use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::workload::WorkloadMetadata;

/// Service layer result type
pub type ServiceResult<T> = anyhow::Result<T>;

/// Information about an experiment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentInfo {
    pub name: String,
    pub path: PathBuf,
    pub store: PathBuf,
    pub workloads: Vec<WorkloadMetadata>,
    /// Unix timestamp (seconds) of the most recent git commit touching the
    /// experiment path. `None` when the path is not a git repository or has
    /// no history.
    #[serde(default)]
    pub last_activity: Option<i64>,
}

/// Options for creating a new experiment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateExperimentOptions {
    pub name: String,
    pub path: Option<PathBuf>,
    pub overwrite: bool,
}

/// Options for running an experiment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunExperimentOptions {
    pub experiment_name: String,
    pub tests: Vec<String>,
    pub short_circuit: bool,
    pub parallel: bool,
    pub params: Vec<(String, String)>,
}

/// Options for visualizing experiment results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizeOptions {
    pub experiment_name: String,
    pub figure_name: String,
    pub tests: Vec<String>,
    pub groupby: Vec<String>,
    pub aggby: Vec<String>,
    pub metric: String,
    pub buckets: Vec<f64>,
    pub max: Option<f64>,
    pub visualization_type: String,
    pub hatched: Vec<usize>,
}

/// Information about the etna configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigInfo {
    pub etna_dir: PathBuf,
    pub store_path: PathBuf,
    pub experiments_path: PathBuf,
    pub repo_dir: PathBuf,
    pub configured: bool,
    pub version: usize,
}

/// Options for integrity check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityCheckOptions {
    pub restore: bool,
    pub remove: bool,
}

/// Result of integrity check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityCheckResult {
    pub faults: Vec<IntegrityFault>,
    pub fixed: bool,
}

/// Integrity fault type
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub enum IntegrityFaultType {
    ExperimentNotFound,
    ExperimentNotDirectory,
}

/// An integrity fault found during check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityFault {
    pub fault_type: IntegrityFaultType,
    pub name: String,
    pub path: PathBuf,
    pub message: String,
}

/// Store query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    pub metrics: Vec<serde_json::Value>,
}

/// A metric to write to the store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteMetricRequest {
    pub hash: String,
    pub data: serde_json::Map<String, serde_json::Value>,
}

/// Options for removing metrics from store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveMetricsOptions {
    pub filter: String,
}

/// Information about a single mutation in a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationInfo {
    /// The name/identifier of the mutation variant
    pub name: String,
    /// Whether this mutation is currently active
    pub active: bool,
    /// The file path where this mutation is defined
    pub file: PathBuf,
    /// The line number where the mutation starts (1-indexed)
    pub line: usize,
    /// The line number where the mutation ends (1-indexed)
    pub end_line: usize,
}

/// Information about mutations in a file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMutationsInfo {
    pub file: PathBuf,
    pub mutations: Vec<MutationInfo>,
}

/// Request to set a mutation variant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetMutationRequest {
    /// The directory path containing mutation files
    pub path: PathBuf,
    /// The variant name to activate
    pub variant: String,
    /// Optional glob pattern to filter files
    pub glob: Option<String>,
}

/// Request to reset mutations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetMutationsRequest {
    /// The directory path to reset mutations in
    pub path: PathBuf,
}

/// Response for mutation operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationOperationResponse {
    pub success: bool,
    pub message: String,
}

/// Information about a test file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestInfo {
    pub name: String,
}
