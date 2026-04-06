use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

use crate::server::error::ServerError;
use crate::server::state::AppState;
use crate::service::store as store_service;
use crate::service::types::{QueryResult, RemoveMetricsOptions, WriteMetricRequest};

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

fn load_experiment_store(
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
    State(state): State<AppState>,
    Query(params): Query<ExperimentParams>,
    Json(request): Json<WriteMetricRequest>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let manager = state.manager.read().unwrap();
    let experiment_name = params
        .experiment
        .ok_or_else(|| ServerError::bad_request("Missing required query parameter: experiment"))?;
    let mut store = load_experiment_store(&manager, &experiment_name)?;

    let count = store_service::write_metric(&mut store, request)?;

    Ok(Json(serde_json::json!({
        "success": true,
        "total_metrics": count
    })))
}

/// Query metrics from the store
pub async fn query_metrics(
    State(state): State<AppState>,
    Query(params): Query<QueryParams>,
) -> Result<Json<QueryResult>, ServerError> {
    let manager = state.manager.read().unwrap();
    let filter = params.filter.unwrap_or_else(|| ".".to_string());

    let result = if let Some(experiment_name) = params.experiment {
        let mut store = load_experiment_store(&manager, &experiment_name)?;
        store_service::load_metrics(&mut store)?;
        store_service::query_metrics(&store, &filter)?
    } else {
        let mut all_metrics = Vec::new();
        for experiment in manager.experiments.values() {
            let mut store = crate::store::Store::new(experiment.store.clone())?;
            store_service::load_metrics(&mut store)?;
            all_metrics.extend(store.metrics);
        }
        let store = crate::store::Store {
            path: std::path::PathBuf::new(),
            metrics: all_metrics,
        };
        store_service::query_metrics(&store, &filter)?
    };

    Ok(Json(result))
}

/// Remove metrics from the store
pub async fn remove_metrics(
    State(state): State<AppState>,
    Query(params): Query<RemoveParams>,
) -> Result<Json<serde_json::Value>, ServerError> {
    let manager = state.manager.read().unwrap();

    let removed_count = if let Some(experiment_name) = params.experiment {
        let mut store = load_experiment_store(&manager, &experiment_name)?;
        store_service::load_metrics(&mut store)?;
        let options = RemoveMetricsOptions {
            filter: params.filter.clone(),
        };
        store_service::remove_metrics(&mut store, options)?
    } else {
        let mut total_removed = 0usize;
        for experiment in manager.experiments.values() {
            let mut store = crate::store::Store::new(experiment.store.clone())?;
            store_service::load_metrics(&mut store)?;
            let options = RemoveMetricsOptions {
                filter: params.filter.clone(),
            };
            total_removed += store_service::remove_metrics(&mut store, options)?;
        }
        total_removed
    };

    Ok(Json(serde_json::json!({
        "success": true,
        "removed_count": removed_count
    })))
}
