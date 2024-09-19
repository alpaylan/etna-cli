use anyhow::Context;

use crate::store::Metric;

pub(crate) fn invoke(experiment_id: String, metric: String) -> anyhow::Result<()> {
    // Get Etna configuration
    let etna_config =
        crate::config::EtnaConfig::get_etna_config().context("Failed to get etna config")?;

    // Load the Store
    let mut store =
        crate::store::Store::load(&etna_config.store_path()).context("Failed to load the store")?;

    // Deserialize the metric
    let data: serde_json::Value = serde_json::from_str(&metric).context(format!(
        "Failed to deserialize the metric as a json string '{}'",
        metric
    ))?;

    // Add the metric to the store
    store.metrics.push(Metric {
        experiment_id,
        data,
    });

    store
        .save(&etna_config.store_path())
        .context("Failed to save the store")?;

    Ok(())
}
