use crate::{experiment::ExperimentMetadata, manager::Manager, service::workload::add_workload};

/// Add a workload to an experiment. `spec` may be either a git URL or a name
/// from the workload catalog — the service layer resolves either.
pub fn invoke(
    mgr: Manager,
    experiment: ExperimentMetadata,
    spec: String,
    reference: Option<String>,
) -> anyhow::Result<()> {
    let result = add_workload(&mgr, &experiment, &spec, reference.as_deref())?;

    tracing::info!(
        "Workload '{}' added to experiment '{}'",
        result.name,
        experiment.name
    );

    Ok(())
}
