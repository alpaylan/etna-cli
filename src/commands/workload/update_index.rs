use crate::service::workload::update_index;

/// Force-refresh the cached workload catalog from the canonical remote URL.
pub fn invoke() -> anyhow::Result<()> {
    let index = update_index()?;
    println!(
        "Refreshed workload catalog: {} entries",
        index.entries.len()
    );
    Ok(())
}
