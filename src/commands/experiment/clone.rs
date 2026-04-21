use std::path::PathBuf;

use crate::{manager::Manager, service::experiment::clone_experiment};

/// Clone a remote experiment repo and register it.
pub fn invoke(
    mut mgr: Manager,
    url: String,
    path: Option<PathBuf>,
    reference: Option<String>,
) -> anyhow::Result<()> {
    let result = clone_experiment(&mut mgr, &url, reference.as_deref(), path)?;

    tracing::info!(
        "Experiment '{}' cloned to '{}'",
        result.name,
        result.path.display()
    );

    Ok(())
}
