use crate::{
    error_context::Context,
    service::{store::write_metric, types::WriteMetricRequest},
    store::Store,
};

/// Write a metric to the store using the service layer.
///
/// The CLI handles argument parsing and delegates to the service layer
/// for the actual metric writing logic.
pub fn invoke(mut store: Store, hash: String, metric: String) -> anyhow::Result<()> {
    // Parse the metric JSON string
    let data: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&metric).context(format!(
            "Failed to deserialize the metric as a json string '{}'",
            metric
        ))?;

    // Convert CLI args to service request
    let request = WriteMetricRequest { hash, data };

    // Call service layer
    write_metric(&mut store, request)?;

    Ok(())
}
