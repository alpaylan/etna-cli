use tabled::settings::{Extract, Style};

use crate::{experiment::ExperimentMetadata, manager::Manager, service::workload as wl_service};

pub fn invoke(_mgr: Manager, experiment: ExperimentMetadata, kind: String) -> anyhow::Result<()> {
    match kind.as_str() {
        "experiment" => {
            let workloads = experiment.workloads();
            let mut rows = vec![("Name",)];
            for workload in workloads.iter() {
                rows.push((workload.name.as_str(),));
            }
            let mut table = tabled::Table::new(rows);
            table
                .with(Extract::segment(1.., ..))
                .with(Style::modern_rounded());
            println!("{}", table);
        }
        "available" => {
            let entries = wl_service::list_available_workloads()?;
            let mut rows: Vec<(&str, &str, &str, String)> =
                vec![("Name", "Language", "Status", "Description".to_string())];
            for e in entries.iter() {
                rows.push((
                    e.name.as_str(),
                    e.language.as_str(),
                    e.status.as_str(),
                    e.description.clone().unwrap_or_default(),
                ));
            }
            let mut table = tabled::Table::new(rows);
            table
                .with(Extract::segment(1.., ..))
                .with(Style::modern_rounded());
            println!("{}", table);
        }
        other => anyhow::bail!("Invalid kind: {}", other),
    }

    Ok(())
}
