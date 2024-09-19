use anyhow::Context;
use tabled::settings::{Extract, Style};

use crate::{
    config::{EtnaConfig, ExperimentConfig},
    workload::Workload,
};

pub(crate) fn invoke(
    experiment_name: Option<String>,
    language: String,
    kind: String,
) -> anyhow::Result<()> {
    // Get etna configuration
    let etna_config = EtnaConfig::get_etna_config().context("Failed to get etna config")?;
    // Get the current experiment
    let experiment_config = experiment_name
        .ok_or(anyhow::anyhow!("No experiment name provided"))
        .and_then(|n| ExperimentConfig::from_etna_config(&n, &etna_config))
        .or_else(|_| ExperimentConfig::from_current_dir())
        .context("No experiment name is provided, and the current directory is not an experiment directory")?;

    match kind.as_str() {
        "experiment" => {
            let mut languages = experiment_config
                .workloads
                .iter()
                .filter(|workload| language == "all" || language == workload.language)
                .collect::<Vec<&Workload>>();

            languages.sort_by(|a, b| a.language.cmp(&b.language).then(a.name.cmp(&b.name)));

            let mut table = vec![("Language", "Name")];
            for workload in languages {
                table.push((workload.language.as_str(), workload.name.as_str()));
            }

            let mut table = tabled::Table::new(table);

            table
                .with(Extract::segment(1.., ..))
                .with(Style::modern_rounded());

            println!("{}", table);
        }
        "available" => {
            anyhow::bail!("'available' kind is not implemented yet");
        }
        _ => {
            anyhow::bail!("Invalid kind: {}", kind);
        }
    }

    Ok(())
}
