use axum::{
    extract::{Path, State},
    Json,
};

use crate::server::error::ServerError;
use crate::server::state::AppState;
use crate::service::job::JobInfo;
use crate::service::store as store_service;
use crate::service::types::QueryResult;

/// List all jobs
pub async fn list_jobs(State(state): State<AppState>) -> Result<Json<Vec<JobInfo>>, ServerError> {
    let jobs = state.job_manager.list_jobs()?;
    Ok(Json(jobs))
}

/// Get a specific job by ID
pub async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<JobInfo>, ServerError> {
    let job = state.job_manager.get_job(&id)?;
    Ok(Json(job))
}

/// Cancel a job
pub async fn cancel_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ServerError> {
    state.job_manager.cancel_job(&id)?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Job '{}' cancelled", id)
    })))
}

/// Get job logs
pub async fn get_job_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<String>>, ServerError> {
    let logs = state.job_manager.get_logs(&id)?;
    Ok(Json(logs))
}

/// Get metrics for a specific job
pub async fn get_job_metrics(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<QueryResult>, ServerError> {
    // Get job info to extract experiment name
    let job = state.job_manager.get_job(&id)?;

    let experiment_name = job
        .metadata
        .get("experiment_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            crate::server::error::ServerError::not_found("Job has no experiment_name in metadata")
        })?;

    // Build a JQ filter to get metrics for this experiment
    // Filter by experiment name
    let filter = format!(r#".[] | select(.experiment == "{}")"#, experiment_name);

    let manager = state.manager.read().unwrap();
    let experiment = manager.get_experiment(experiment_name).ok_or_else(|| {
        ServerError::not_found(format!("Experiment not found: {experiment_name}"))
    })?;

    let mut store = crate::store::Store::new(experiment.store)?;
    store_service::load_metrics(&mut store)?;
    let result = store_service::query_metrics(&store, &filter)?;

    Ok(Json(result))
}
