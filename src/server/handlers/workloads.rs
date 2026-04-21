use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::server::error::ServerError;
use crate::server::state::AppState;
use crate::service::workload as wl_service;
use crate::workload::WorkloadMetadata;
use crate::workload_index::WorkloadEntry;

/// Request body for adding a workload.
#[derive(Debug, Deserialize)]
pub struct AddWorkloadRequest {
    /// Catalog name or git URL of the workload repo. When a name is supplied,
    /// the server resolves it against the cached index.
    /// `url` is accepted as an alias for backwards compatibility with older
    /// clients that only know how to send URLs.
    #[serde(alias = "url")]
    pub spec: String,
    /// Optional branch/tag/ref. When omitted, the catalog's `default_ref`
    /// (if any) wins; failing that, the repo's default branch.
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
}

/// Response for adding a workload
#[derive(Debug, Serialize)]
pub struct AddWorkloadResponse {
    pub workload: WorkloadMetadata,
}

/// Response for refreshing the workload catalog
#[derive(Debug, Serialize)]
pub struct RefreshWorkloadIndexResponse {
    pub refreshed: bool,
    pub entries: usize,
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
        &request.spec,
        request.reference.as_deref(),
    )?;

    Ok(Json(AddWorkloadResponse { workload }))
}

/// List the available workloads in the catalog (cached copy, no network).
pub async fn list_available_workloads() -> Result<Json<Vec<WorkloadEntry>>, ServerError> {
    let entries = wl_service::list_available_workloads()?;
    Ok(Json(entries))
}

/// Refresh the cached workload catalog from the canonical URL. Runs the
/// blocking HTTP call on a tokio thread pool.
pub async fn refresh_workload_index() -> Result<Json<RefreshWorkloadIndexResponse>, ServerError> {
    let index = tokio::task::spawn_blocking(wl_service::update_index)
        .await
        .map_err(|e| ServerError::internal(format!("refresh join failed: {e}")))??;
    Ok(Json(RefreshWorkloadIndexResponse {
        refreshed: true,
        entries: index.entries.len(),
    }))
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
