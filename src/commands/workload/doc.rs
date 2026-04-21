use std::path::PathBuf;

use crate::{service::workload as wl_service, workload::WorkloadManifest};

/// Regenerate `BUGS.md` and `TASKS.md` from `<dir>/etna.toml`.
pub fn invoke(dir: PathBuf) -> anyhow::Result<()> {
    let manifest = WorkloadManifest::read(&dir)?;
    let wrote = wl_service::write_docs(&manifest, &dir)?;
    if wrote {
        tracing::info!(
            "Regenerated BUGS.md and TASKS.md for workload '{}'",
            manifest.name
        );
    }
    Ok(())
}
