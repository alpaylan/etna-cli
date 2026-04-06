use std::path::PathBuf;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::server::error::ServerError;
use crate::server::state::AppState;
use crate::service::experiment as exp_service;
use crate::service::job::JobStatus;
use crate::service::types::{
    CreateExperimentOptions, ExperimentInfo, RunExperimentOptions, TestInfo,
};

/// Request body for creating an experiment
#[derive(Debug, Deserialize)]
pub struct CreateExperimentRequest {
    pub name: String,
    pub path: Option<String>,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(default)]
    pub register: bool,
}

/// Response for creating an experiment
#[derive(Debug, Serialize)]
pub struct CreateExperimentResponse {
    pub experiment: ExperimentInfo,
}

/// Request body for running an experiment
#[derive(Debug, Deserialize)]
pub struct RunExperimentRequest {
    pub tests: Vec<String>,
    #[serde(default)]
    pub short_circuit: bool,
    #[serde(default)]
    pub parallel: bool,
    #[serde(default)]
    pub params: Vec<(String, String)>,
}

/// Response for running an experiment (async job)
#[derive(Debug, Serialize)]
pub struct RunExperimentResponse {
    pub job_id: String,
    pub status: String,
}

/// Request body for visualization
#[derive(Debug, Deserialize)]
pub struct VisualizeRequest {
    pub figure_name: String,
    pub tests: Vec<String>,
    #[serde(default)]
    pub groupby: Vec<String>,
    #[serde(default)]
    pub aggby: Vec<String>,
    #[serde(default = "default_metric")]
    pub metric: String,
    #[serde(default)]
    pub buckets: Vec<f64>,
    pub max: Option<f64>,
    #[serde(default = "default_visualization_type")]
    pub visualization_type: String,
    #[serde(default)]
    pub hatched: Vec<usize>,
}

fn default_metric() -> String {
    "time".to_string()
}

fn default_visualization_type() -> String {
    "bucket".to_string()
}

/// List all experiments
pub async fn list_experiments(
    State(state): State<AppState>,
) -> Result<Json<Vec<ExperimentInfo>>, ServerError> {
    let manager = state.manager.read().unwrap();
    let experiments = exp_service::list_experiments(&manager)?;
    Ok(Json(experiments))
}

/// Get a specific experiment by name
pub async fn get_experiment(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ExperimentInfo>, ServerError> {
    let manager = state.manager.read().unwrap();
    let experiment = exp_service::get_experiment(&manager, &name)?;
    Ok(Json(experiment))
}

/// Create a new experiment
pub async fn create_experiment(
    State(state): State<AppState>,
    Json(request): Json<CreateExperimentRequest>,
) -> Result<Json<CreateExperimentResponse>, ServerError> {
    let mut manager = state.manager.write().unwrap();

    let options = CreateExperimentOptions {
        name: request.name,
        path: request.path.map(PathBuf::from),
        overwrite: request.overwrite,
        register: request.register,
    };

    let experiment = exp_service::create_experiment(&mut manager, options)?;

    Ok(Json(CreateExperimentResponse { experiment }))
}

/// Delete an experiment
pub async fn delete_experiment(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let mut manager = state.manager.write().unwrap();

    // By default don't delete files - require explicit parameter
    exp_service::delete_experiment(&mut manager, &name, false)?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Experiment '{}' deleted", name)
    })))
}

/// Run an experiment (starts async job)
pub async fn run_experiment(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<RunExperimentRequest>,
) -> Result<Json<RunExperimentResponse>, ServerError> {
    // Verify the experiment exists
    {
        let manager = state.manager.read().unwrap();
        let _ = exp_service::get_experiment(&manager, &name)?;
    }

    let options = RunExperimentOptions {
        experiment_name: name.clone(),
        tests: request.tests,
        short_circuit: request.short_circuit,
        parallel: request.parallel,
        params: request.params,
    };

    // Create a job for the experiment run
    let job_id = state.job_manager.create_experiment_run_job(&options)?;

    // Get the cancel flag for this job
    let cancel_flag = state.job_manager.get_cancel_flag(&job_id)?;

    // Clone what we need for the async task
    let state_clone = state.clone();
    let job_id_clone = job_id.clone();
    let options_clone = options.clone();

    // Spawn the experiment run in a background task
    tokio::spawn(async move {
        let is_cancelled = || cancel_flag.read().map(|flag| *flag).unwrap_or(false);

        if is_cancelled() {
            let _ = state_clone
                .job_manager
                .update_job_status(&job_id_clone, JobStatus::Cancelled);
            return;
        }

        // Update job status to running
        let _ = state_clone
            .job_manager
            .update_job_status(&job_id_clone, JobStatus::Running);

        if is_cancelled() {
            let _ = state_clone
                .job_manager
                .update_job_status(&job_id_clone, JobStatus::Cancelled);
            return;
        }

        // Get the manager and bind it to the experiment-local store.
        let manager = {
            let mgr = state_clone.manager.read().unwrap();
            let Some(experiment) = mgr.get_experiment(&options_clone.experiment_name) else {
                let _ = state_clone.job_manager.set_job_error(
                    &job_id_clone,
                    format!("Experiment not found: {}", options_clone.experiment_name),
                );
                return;
            };

            let mut manager = crate::manager::Manager {
                experiments: mgr.experiments.clone(),
                store: None,
                config: crate::config::EtnaConfig::get_etna_config().unwrap(),
            };
            if let Err(e) = manager.set_store_path(experiment.store.clone()) {
                let _ = state_clone
                    .job_manager
                    .set_job_error(&job_id_clone, e.to_string());
                return;
            }
            manager
        };

        // Run the experiment on a blocking thread with cancel support.
        let cancel_for_run = cancel_flag.clone();
        let result = tokio::task::spawn_blocking(move || {
            exp_service::run_experiment(manager, options_clone, Some(cancel_for_run))
        })
        .await;

        match result {
            Ok(Ok(_)) => {
                if is_cancelled() {
                    let _ = state_clone
                        .job_manager
                        .update_job_status(&job_id_clone, JobStatus::Cancelled);
                } else {
                    let _ = state_clone
                        .job_manager
                        .update_job_status(&job_id_clone, JobStatus::Completed);
                }
            }
            Ok(Err(e)) => {
                if is_cancelled() || e.to_string().to_lowercase().contains("cancel") {
                    let _ = state_clone
                        .job_manager
                        .update_job_status(&job_id_clone, JobStatus::Cancelled);
                } else {
                    let _ = state_clone
                        .job_manager
                        .set_job_error(&job_id_clone, e.to_string());
                }
            }
            Err(e) => {
                if is_cancelled() {
                    let _ = state_clone
                        .job_manager
                        .update_job_status(&job_id_clone, JobStatus::Cancelled);
                } else {
                    let _ = state_clone
                        .job_manager
                        .set_job_error(&job_id_clone, format!("Experiment task failed: {e}"));
                }
            }
        }
    });

    Ok(Json(RunExperimentResponse {
        job_id,
        status: "pending".to_string(),
    }))
}

/// List available tests for an experiment
pub async fn list_tests(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<TestInfo>>, ServerError> {
    let manager = state.manager.read().unwrap();
    let experiment = manager
        .get_experiment(&name)
        .ok_or_else(|| ServerError::not_found(format!("Experiment not found: {}", name)))?;

    let tests = exp_service::list_tests(&experiment.path)?;
    Ok(Json(tests))
}

/// Generate visualization (placeholder - synchronous for now)
pub async fn visualize(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<VisualizeRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    // Verify the experiment exists
    let experiment = {
        let manager = state.manager.read().unwrap();
        manager
            .get_experiment(&name)
            .ok_or_else(|| ServerError::not_found(format!("Experiment not found: {}", name)))?
    };

    // For now, return a message that this would generate visualization
    // Full implementation would call the visualize service
    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!(
            "Visualization '{}' would be generated for experiment '{}'",
            request.figure_name,
            experiment.name
        ),
        "figure_name": request.figure_name,
        "tests": request.tests,
        "visualization_type": request.visualization_type
    })))
}

/// Test definition for API serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDefinition {
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
    pub tasks: Vec<std::collections::HashMap<String, String>>,
}

impl From<crate::experiment::Test> for TestDefinition {
    fn from(test: crate::experiment::Test) -> Self {
        Self {
            language: test.language,
            workload: test.workload,
            trials: test.trials,
            timeout: test.timeout,
            mutations: test.mutations,
            cross: test.cross,
            params: test.params,
            tasks: test.tasks,
        }
    }
}

impl From<TestDefinition> for crate::experiment::Test {
    fn from(def: TestDefinition) -> Self {
        Self {
            language: def.language,
            workload: def.workload,
            trials: def.trials,
            timeout: def.timeout,
            mutations: def.mutations,
            cross: def.cross,
            params: def.params,
            tasks: def.tasks,
        }
    }
}

/// Path parameters for test endpoints
#[derive(Debug, Deserialize)]
pub struct TestPathParams {
    pub name: String,
    pub test_name: String,
}

/// Get a specific test's content
pub async fn get_test(
    State(state): State<AppState>,
    Path(params): Path<TestPathParams>,
) -> Result<Json<Vec<TestDefinition>>, ServerError> {
    let manager = state.manager.read().unwrap();
    let experiment = manager
        .get_experiment(&params.name)
        .ok_or_else(|| ServerError::not_found(format!("Experiment not found: {}", params.name)))?;

    let tests = exp_service::get_test_content(&experiment.path, &params.test_name)?;
    let definitions: Vec<TestDefinition> = tests.into_iter().map(|t| t.into()).collect();

    Ok(Json(definitions))
}

/// Save a test file
pub async fn save_test(
    State(state): State<AppState>,
    Path(params): Path<TestPathParams>,
    Json(definitions): Json<Vec<TestDefinition>>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let manager = state.manager.read().unwrap();
    let experiment = manager
        .get_experiment(&params.name)
        .ok_or_else(|| ServerError::not_found(format!("Experiment not found: {}", params.name)))?;

    let tests: Vec<crate::experiment::Test> = definitions.into_iter().map(|d| d.into()).collect();
    exp_service::save_test(&experiment.path, &params.test_name, &tests)?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Test '{}' saved", params.test_name)
    })))
}

/// Delete a test file
pub async fn delete_test(
    State(state): State<AppState>,
    Path(params): Path<TestPathParams>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let manager = state.manager.read().unwrap();
    let experiment = manager
        .get_experiment(&params.name)
        .ok_or_else(|| ServerError::not_found(format!("Experiment not found: {}", params.name)))?;

    exp_service::delete_test(&experiment.path, &params.test_name)?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Test '{}' deleted", params.test_name)
    })))
}
