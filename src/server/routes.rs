use axum::{
    routing::{delete, get, post, put},
    Router,
};

use super::handlers;
use super::state::AppState;

/// Create the API routes
pub fn api_routes() -> Router<AppState> {
    Router::new()
        // Health check
        .route("/api/v1/health", get(handlers::health::health_check))
        // Experiments
        .route(
            "/api/v1/experiments",
            get(handlers::experiments::list_experiments),
        )
        .route(
            "/api/v1/experiments",
            post(handlers::experiments::create_experiment),
        )
        .route(
            "/api/v1/experiments/clone",
            post(handlers::experiments::clone_experiment),
        )
        .route(
            "/api/v1/experiments/{name}",
            get(handlers::experiments::get_experiment),
        )
        .route(
            "/api/v1/experiments/{name}",
            delete(handlers::experiments::delete_experiment),
        )
        .route(
            "/api/v1/experiments/{name}/run",
            post(handlers::experiments::run_experiment),
        )
        .route(
            "/api/v1/experiments/{name}/tests",
            get(handlers::experiments::list_tests),
        )
        .route(
            "/api/v1/experiments/{name}/tests/{test_name}",
            get(handlers::experiments::get_test),
        )
        .route(
            "/api/v1/experiments/{name}/tests/{test_name}",
            put(handlers::experiments::save_test),
        )
        .route(
            "/api/v1/experiments/{name}/tests/{test_name}",
            delete(handlers::experiments::delete_test),
        )
        .route(
            "/api/v1/experiments/{name}/visualize",
            post(handlers::experiments::visualize),
        )
        .route(
            "/api/v1/experiments/{name}/report",
            get(handlers::experiments::get_report),
        )
        // Workloads
        .route(
            "/api/v1/workloads/available",
            get(handlers::workloads::list_available_workloads),
        )
        .route(
            "/api/v1/workloads/index/refresh",
            post(handlers::workloads::refresh_workload_index),
        )
        .route(
            "/api/v1/experiments/{name}/workloads",
            get(handlers::workloads::list_workloads),
        )
        .route(
            "/api/v1/experiments/{name}/workloads",
            post(handlers::workloads::add_workload),
        )
        .route(
            "/api/v1/experiments/{name}/workloads/{workload}",
            delete(handlers::workloads::remove_workload),
        )
        // Store
        .route("/api/v1/store/write", post(handlers::store::write_metric))
        .route("/api/v1/store/query", get(handlers::store::query_metrics))
        .route("/api/v1/store", delete(handlers::store::remove_metrics))
        // System
        .route("/api/v1/config", get(handlers::system::get_config))
        .route("/api/v1/setup", post(handlers::system::run_setup))
        .route("/api/v1/check", post(handlers::system::integrity_check))
        // Jobs
        .route("/api/v1/jobs", get(handlers::jobs::list_jobs))
        .route("/api/v1/jobs/{id}", get(handlers::jobs::get_job))
        .route("/api/v1/jobs/{id}/cancel", post(handlers::jobs::cancel_job))
        .route("/api/v1/jobs/{id}/logs", get(handlers::jobs::get_job_logs))
        .route(
            "/api/v1/jobs/{id}/metrics",
            get(handlers::jobs::get_job_metrics),
        )
        // Mutations
        .route(
            "/api/v1/experiments/{name}/workloads/{workload}/mutations",
            get(handlers::mutations::get_workload_mutations),
        )
        .route(
            "/api/v1/mutations",
            get(handlers::mutations::list_mutations),
        )
        .route(
            "/api/v1/mutations/file",
            get(handlers::mutations::get_file_mutations),
        )
        .route(
            "/api/v1/mutations/set",
            post(handlers::mutations::set_mutation),
        )
        .route(
            "/api/v1/mutations/reset",
            post(handlers::mutations::reset_mutations),
        )
}
