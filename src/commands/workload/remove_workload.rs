use crate::{experiment::ExperimentMetadata, service::workload::remove_workload};

/// Remove a workload from an experiment by name.
pub fn invoke(experiment: ExperimentMetadata, workload: String) -> anyhow::Result<()> {
    remove_workload(&experiment, &workload)?;

    tracing::info!(
        "Workload '{}' removed from experiment '{}'",
        workload,
        experiment.name
    );

    Ok(())
}
