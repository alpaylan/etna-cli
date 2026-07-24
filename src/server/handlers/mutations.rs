use std::path::PathBuf;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::server::error::ServerError;
use crate::server::state::AppState;
use crate::service::mutations as mutation_service;
use crate::service::types::{
    FileMutationsInfo, MutationOperationResponse, ResetMutationsRequest, SetMutationRequest,
};

/// Query parameters for listing mutations
#[derive(Debug, Deserialize)]
pub struct ListMutationsQuery {
    pub path: String,
}

/// Query parameters for getting file mutations
#[derive(Debug, Deserialize)]
pub struct FileMutationsQuery {
    pub path: String,
}

/// Resolve a caller-supplied filesystem path and require it to live inside a
/// registered experiment's directory. The mutation endpoints hand the path to
/// marauders, which reads or rewrites files there — without this check any
/// HTTP client could enumerate or mutate arbitrary paths the server user can
/// touch.
fn confine_to_registered(state: &AppState, path: &std::path::Path) -> Result<PathBuf, ServerError> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| ServerError::not_found(format!("Path not found: {}", path.display())))?;
    let manager = state.manager.read().unwrap();
    let inside_registered = manager.experiments.values().any(|meta| {
        std::fs::canonicalize(&meta.path).is_ok_and(|exp| canonical.starts_with(&exp))
    });
    if inside_registered {
        Ok(canonical)
    } else {
        Err(ServerError::bad_request(format!(
            "Path is not inside a registered experiment: {}",
            path.display()
        )))
    }
}

/// List all mutations in a directory
pub async fn list_mutations(
    State(state): State<AppState>,
    Query(query): Query<ListMutationsQuery>,
) -> Result<Json<Vec<FileMutationsInfo>>, ServerError> {
    let path = confine_to_registered(&state, &PathBuf::from(&query.path))?;
    let mutations = mutation_service::list_mutations(&path)?;
    Ok(Json(mutations))
}

/// Get mutations for a specific file with line locations
pub async fn get_file_mutations(
    State(state): State<AppState>,
    Query(query): Query<FileMutationsQuery>,
) -> Result<Json<FileMutationsInfo>, ServerError> {
    let path = confine_to_registered(&state, &PathBuf::from(&query.path))?;
    let mutations = mutation_service::get_file_mutations(&path)?;
    Ok(Json(mutations))
}

/// Set a mutation variant as active
pub async fn set_mutation(
    State(state): State<AppState>,
    Json(request): Json<SetMutationRequest>,
) -> Result<Json<MutationOperationResponse>, ServerError> {
    let path = confine_to_registered(&state, &request.path)?;

    mutation_service::set_mutation(&path, &request.variant, request.glob.as_deref())?;

    Ok(Json(MutationOperationResponse {
        success: true,
        message: format!("Activated mutation variant: {}", request.variant),
    }))
}

/// List mutation variant names available for a specific workload within an
/// experiment. Returns a deduplicated, sorted list with "base" pinned first.
pub async fn get_workload_mutations(
    State(state): State<AppState>,
    Path((experiment_name, workload)): Path<(String, String)>,
) -> Result<Json<Vec<String>>, ServerError> {
    let workload_path = {
        let manager = state.manager.read().unwrap();
        let experiment = manager.get_experiment(&experiment_name).ok_or_else(|| {
            ServerError::not_found(format!("Experiment not found: {}", experiment_name))
        })?;
        experiment.workload_path(&workload).ok_or_else(|| {
            ServerError::not_found(format!(
                "Workload '{}' not found in experiment '{}'",
                workload, experiment_name
            ))
        })?
    };

    let files = mutation_service::list_mutations(&workload_path)?;
    let mut names: Vec<String> = files
        .iter()
        .flat_map(|f| f.mutations.iter().map(|m| m.name.clone()))
        .collect();
    names.sort();
    names.dedup();
    names.retain(|n| n != "base");
    names.insert(0, "base".to_string());

    Ok(Json(names))
}

/// Reset all mutations in a directory
pub async fn reset_mutations(
    State(state): State<AppState>,
    Json(request): Json<ResetMutationsRequest>,
) -> Result<Json<MutationOperationResponse>, ServerError> {
    let path = confine_to_registered(&state, &request.path)?;

    mutation_service::reset_mutations(&path)?;

    Ok(Json(MutationOperationResponse {
        success: true,
        message: format!("Reset mutations in: {}", request.path.display()),
    }))
}
