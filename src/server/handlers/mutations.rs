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

/// List all mutations in a directory
pub async fn list_mutations(
    Query(query): Query<ListMutationsQuery>,
) -> Result<Json<Vec<FileMutationsInfo>>, ServerError> {
    let path = PathBuf::from(&query.path);

    if !path.exists() {
        return Err(ServerError::not_found(format!(
            "Path not found: {}",
            query.path
        )));
    }

    let mutations = mutation_service::list_mutations(&path)?;
    Ok(Json(mutations))
}

/// Get mutations for a specific file with line locations
pub async fn get_file_mutations(
    Query(query): Query<FileMutationsQuery>,
) -> Result<Json<FileMutationsInfo>, ServerError> {
    let path = PathBuf::from(&query.path);

    if !path.exists() {
        return Err(ServerError::not_found(format!(
            "File not found: {}",
            query.path
        )));
    }

    let mutations = mutation_service::get_file_mutations(&path)?;
    Ok(Json(mutations))
}

/// Set a mutation variant as active
pub async fn set_mutation(
    Json(request): Json<SetMutationRequest>,
) -> Result<Json<MutationOperationResponse>, ServerError> {
    if !request.path.exists() {
        return Err(ServerError::not_found(format!(
            "Path not found: {}",
            request.path.display()
        )));
    }

    mutation_service::set_mutation(&request.path, &request.variant, request.glob.as_deref())?;

    Ok(Json(MutationOperationResponse {
        success: true,
        message: format!("Activated mutation variant: {}", request.variant),
    }))
}

/// List mutation variant names available for a specific (language, workload)
/// pair. Resolves the workload directory from the configured repo_dir and
/// returns a deduplicated, sorted list of names with "base" pinned first.
pub async fn get_workload_mutations(
    State(state): State<AppState>,
    Path((language, workload)): Path<(String, String)>,
) -> Result<Json<Vec<String>>, ServerError> {
    let repo_dir = {
        let manager = state.manager.read().unwrap();
        manager.config.repo_dir()
    };
    let workload_path = repo_dir.join("workloads").join(&language).join(&workload);

    if !workload_path.exists() {
        return Err(ServerError::not_found(format!(
            "Workload not found at {}",
            workload_path.display()
        )));
    }

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
    Json(request): Json<ResetMutationsRequest>,
) -> Result<Json<MutationOperationResponse>, ServerError> {
    if !request.path.exists() {
        return Err(ServerError::not_found(format!(
            "Path not found: {}",
            request.path.display()
        )));
    }

    mutation_service::reset_mutations(&request.path)?;

    Ok(Json(MutationOperationResponse {
        success: true,
        message: format!("Reset mutations in: {}", request.path.display()),
    }))
}
