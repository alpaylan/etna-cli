use tabled::settings::{Extract, Style};

use crate::{
    experiment::ExperimentMetadata, manager::Manager, service::workload as wl_service,
    workload::WorkloadMetadata,
};

pub fn invoke(
    _mgr: Manager,
    experiment: ExperimentMetadata,
    kind: String,
) -> anyhow::Result<()> {
    let rows: Vec<WorkloadMetadata> = match kind.as_str() {
        "experiment" => experiment.workloads(),
        "available" => wl_service::list_available_workloads()?,
        other => anyhow::bail!("Invalid kind: {}", other),
    };

    let mut table = vec![("Name",)];
    for workload in rows.iter() {
        table.push((workload.name.as_str(),));
    }

    let mut table = tabled::Table::new(table);

    table
        .with(Extract::segment(1.., ..))
        .with(Style::modern_rounded());

    println!("{}", table);

    Ok(())
}
