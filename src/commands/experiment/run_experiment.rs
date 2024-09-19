use anyhow::Context;
use log::{info, warn};

use crate::{
    config::{self, EtnaConfig},
    python_driver,
    store::Store,
};

pub(crate) fn invoke(name: Option<String>) -> anyhow::Result<()> {
    let etna_config = EtnaConfig::get_etna_config()?;
    let experiment_config = name
        .context("No experiment name provided")
        .and_then(|n| config::ExperimentConfig::from_etna_config(&n, &etna_config))
        .or_else(|_| config::ExperimentConfig::from_current_dir())?;

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
