use crate::{experiment::ExperimentMetadata, manager::Manager, service::workload::add_workload};

/// Add a remote workload (git URL) to an experiment via the service layer.
pub fn invoke(
    mgr: Manager,
    experiment: ExperimentMetadata,
    url: String,
    reference: Option<String>,
) -> anyhow::Result<()> {
    let result = add_workload(&mgr, &experiment, &url, reference.as_deref())?;

    tracing::info!(
        "Workload '{}' added to experiment '{}'",
        result.name,
        experiment.name
    );

    Ok(())
}
