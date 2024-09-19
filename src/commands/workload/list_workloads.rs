use std::collections::HashMap;

use anyhow::Context;

use crate::config::{EtnaConfig, ExperimentConfig};

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
                .map(|workload| (workload.language.clone(), vec![]))
                .collect::<HashMap<String, Vec<String>>>();

            for workload in experiment_config.workloads {
                languages
                    .get_mut(&workload.language)
                    .unwrap()
                    .push(workload.name);
            }

            let languages = languages
                .iter()
                .filter(|(lang, _)| language == "all" || language == **lang)
                .collect::<HashMap<&String, &Vec<String>>>();

            for (lang, workloads) in languages {
                println!("{}:", lang);
                for workload in workloads {
                    println!("\t{}", workload);
                }
            }
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
