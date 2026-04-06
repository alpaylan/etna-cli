use std::path::PathBuf;

use crate::{
    manager::Manager,
    service::{experiment::create_experiment, types::CreateExperimentOptions},
};

/// Create a new experiment using the service layer.
///
/// The CLI handles argument parsing and delegates to the service layer
/// for the actual experiment creation logic.
pub fn invoke(
    mut mgr: Manager,
    name: String,
    path: Option<PathBuf>,
    overwrite: bool,
) -> anyhow::Result<()> {
    // Convert CLI args to service options
    let options = CreateExperimentOptions {
        name,
        path,
        overwrite,
    };

    // Call service layer
    let result = create_experiment(&mut mgr, options)?;

    // Output is already logged by the service layer
    tracing::info!(
        "Experiment '{}' created at '{}'",
        result.name,
        result.path.display()
    );

    Ok(())
}
