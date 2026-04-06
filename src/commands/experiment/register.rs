use std::path::PathBuf;

use crate::{manager::Manager, service::experiment::register_experiment};

/// Register an existing experiment using the service layer.
pub fn invoke(mut mgr: Manager, name: Option<String>, path: Option<PathBuf>) -> anyhow::Result<()> {
    let result = register_experiment(&mut mgr, name, path)?;

    tracing::info!(
        "Experiment '{}' registered at '{}'",
        result.name,
        result.path.display()
    );

    Ok(())
}
