use anyhow::Context;
use log::{info, warn};

use crate::{
    config::{EtnaConfig, ExperimentConfig},
    python_driver,
    store::Store,
};

pub(crate) fn invoke(experiment_name: Option<String>) -> anyhow::Result<()> {
    let etna_config = EtnaConfig::get_etna_config()?;

    let experiment_config = experiment_name
        .ok_or(anyhow::anyhow!("No experiment name provided"))
        .and_then(|n| ExperimentConfig::from_etna_config(&n, &etna_config))
        .or_else(|_| ExperimentConfig::from_current_dir())
        .context("No experiment name is provided, and the current directory is not an experiment directory")?;

    let mut store = Store::load(&etna_config.store_path())?;

    let snapshot = Store::take_snapshot(&mut store, &etna_config, &experiment_config)?;

    let experiment = store.get_experiment(&experiment_config.name)?;

    info!(
        "Taking snapshot for the experiment {}",
        experiment_config.name
    );

    if snapshot != experiment.snapshot {
        warn!(
            "Updating snapshot for the experiment {}",
            experiment_config.name
        );
        let experiment = experiment.with_snapshot(snapshot.clone());
        store.experiments.insert(experiment);
    }

    python_driver::run_experiment(&experiment_config, snapshot)?;

    Ok(())
}
