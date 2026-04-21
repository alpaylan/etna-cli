use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::server::error::ServerError;
use crate::server::state::AppState;
use crate::service::workload as wl_service;
use crate::workload::WorkloadMetadata;

/// Request body for adding a remote workload.
#[derive(Debug, Deserialize)]
pub struct AddWorkloadRequest {
    /// Git URL of the workload repo. Must contain `etna.toml` + `steps.json`
    /// at its root.
    pub url: String,
    /// Optional branch/tag/ref. When omitted, the repo's default branch is
    /// used.
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
}

/// Response for adding a workload
#[derive(Debug, Serialize)]
pub struct AddWorkloadResponse {
    pub workload: WorkloadMetadata,
}

/// List workloads in an experiment
pub async fn list_workloads(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<WorkloadMetadata>>, ServerError> {
    let manager = state.manager.read().unwrap();

    let experiment = manager
        .get_experiment(&name)
        .ok_or_else(|| ServerError::not_found(format!("Experiment not found: {}", name)))?;

    let workloads = wl_service::list_workloads(&experiment)?;

    Ok(Json(workloads))
}

/// Add a workload to an experiment
pub async fn add_workload(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(request): Json<AddWorkloadRequest>,
) -> Result<Json<AddWorkloadResponse>, ServerError> {
    let manager = state.manager.read().unwrap();

    let experiment = manager
        .get_experiment(&name)
        .ok_or_else(|| ServerError::not_found(format!("Experiment not found: {}", name)))?;

    let workload = wl_service::add_workload(
        &manager,
        &experiment,
        &request.url,
        request.reference.as_deref(),
    )?;

    Ok(Json(AddWorkloadResponse { workload }))
}

/// List workloads available to add. MVP returns an empty list; a catalog can
/// be wired in later without changing the route.
pub async fn list_available_workloads() -> Result<Json<Vec<WorkloadMetadata>>, ServerError> {
    let workloads = wl_service::list_available_workloads()?;
    Ok(Json(workloads))
}

/// Remove a workload from an experiment
pub async fn remove_workload(
    State(state): State<AppState>,
    Path((name, wl)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let manager = state.manager.read().unwrap();

    let experiment = manager
        .get_experiment(&name)
        .ok_or_else(|| ServerError::not_found(format!("Experiment not found: {}", name)))?;

    wl_service::remove_workload(&experiment, &wl)?;

    Ok(Json(serde_json::json!({
        "success": true,
        "message": format!("Workload '{}' removed from experiment '{}'", wl, name)
    })))
}
