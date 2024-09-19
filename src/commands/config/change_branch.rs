use anyhow::Context;
use log::info;

use crate::{git_driver, python_driver};

pub(crate) fn invoke(branch: String) -> anyhow::Result<()> {
    // Get etna configuration
    let mut etna_config = crate::config::EtnaConfig::get_etna_config()?;

    // Check the current etna branch
    if etna_config.branch == branch {
        info!("The etna repository is already on the '{}' branch", branch);
        return Ok(());
    }

    // Change the branch
    git_driver::change_branch(&etna_config.repo_dir, &branch)?;
    info!("Changed the etna repository branch to '{}'", branch);

    // Recompiling the etna repository
    python_driver::make(&etna_config).context(format!(
        "Failed to recompile ETNA with the new branch '{}'",
        branch
    ))?;

    // Update the etna configuration
    etna_config.branch = branch;

    // Save the etna configuration
    etna_config.save()?;

    Ok(())
}
