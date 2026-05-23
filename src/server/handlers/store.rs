use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

use crate::server::error::ServerError;
use crate::server::state::AppState;
use crate::service::types::{QueryResult, WriteMetricRequest};

#[derive(Debug, Deserialize)]
pub struct ExperimentParams {
    pub experiment: Option<String>,
}

/// Query parameters for store query
#[derive(Debug, Deserialize)]
pub struct QueryParams {
    pub experiment: Option<String>,
    pub filter: Option<String>,
}

/// Query parameters for removing metrics
#[derive(Debug, Deserialize)]
pub struct RemoveParams {
    pub experiment: Option<String>,
    pub filter: String,
}

fn _load_experiment_store(
    manager: &crate::manager::Manager,
    experiment_name: &str,
) -> Result<crate::store::Store, ServerError> {
    let experiment = manager.get_experiment(experiment_name).ok_or_else(|| {
        ServerError::not_found(format!("Experiment not found: {experiment_name}"))
    })?;
    let store = crate::store::Store::new(experiment.store)?;
    Ok(store)
}

/// Write a metric to the store
pub async fn write_metric(
    State(_state): State<AppState>,
    Query(_params): Query<ExperimentParams>,
    Json(_request): Json<WriteMetricRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    todo!()
}

/// Query metrics from the store
pub async fn query_metrics(
    State(_state): State<AppState>,
    Query(_params): Query<QueryParams>,
) -> Result<Json<QueryResult>, ServerError> {
    todo!()
}

/// Remove metrics from the store
pub async fn remove_metrics(
    State(_state): State<AppState>,
    Query(_params): Query<RemoveParams>,
) -> Result<Json<serde_json::Value>, ServerError> {
    todo!()
}
