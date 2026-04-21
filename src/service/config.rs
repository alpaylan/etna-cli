use tracing::info;

use crate::{
    config::{current_version, EtnaConfig},
    error_context::Context,
};

use super::types::{ConfigInfo, ServiceResult};

/// Get the current configuration
pub fn get_config() -> ServiceResult<ConfigInfo> {
    let etna_config = EtnaConfig::get_etna_config().context("Failed to get etna config")?;

    Ok(ConfigInfo {
        etna_dir: etna_config.etna_dir.clone(),
        store_path: etna_config.store_path(),
        experiments_path: etna_config.experiments_path(),
        configured: etna_config.configured,
        version: etna_config.version,
    })
}

/// Run the etna setup process
pub fn setup(overwrite: bool) -> ServiceResult<ConfigInfo> {
    info!("Setting up etna...");

    // Get the home directory
    let etna_dir = EtnaConfig::get_etna_dir().context("Failed to get .etna directory")?;

    // If `.etna` directory does not exist, create it
    if !etna_dir.exists() {
        std::fs::create_dir(&etna_dir).context("Failed to create .etna directory")?;
    }

    // Check if the `config.json` file exists
    let config_path = etna_dir.join("config.json");
    // If it exists, read the configuration, otherwise create it
    let mut config = if let Ok(file) = std::fs::File::open(&config_path) {
        serde_json::from_reader(file).context("Failed to read config.json")?
    } else {
        let default_config = EtnaConfig::new().context("Failed to create default config")?;
        let file = std::fs::File::create(&config_path).context("Failed to create config.json")?;
        serde_json::to_writer_pretty(file, &default_config)
            .context("Failed to write to config.json")?;
        default_config
    };

    if config.configured && config.version == current_version() && !overwrite {
        // If etna is already configured, return
        info!("etna is already configured");
        return Ok(ConfigInfo {
            etna_dir: config.etna_dir.clone(),
            store_path: config.store_path(),
            experiments_path: config.experiments_path(),
            configured: config.configured,
            version: config.version,
        });
    }

    // Create the `experiments.json` file
    let experiments_path = config.experiments_path();
    if !experiments_path.exists() {
        info!(
            "Creating experiments tracking file at {}",
            experiments_path.display()
        );
        let file = std::fs::File::create(&experiments_path)
            .with_context(|| format!("Failed to create '{}'", experiments_path.display()))?;
        serde_json::to_writer_pretty(file, &serde_json::json!({}))
            .with_context(|| format!("Failed to write to '{}'", experiments_path.display()))?;
    }

    // Mark etna as configured in the config.json file
    config.configured = true;
    let file = std::fs::File::create(&config_path).context("Failed to create config.json")?;
    serde_json::to_writer_pretty(file, &config).context("Failed to write to config.json")?;

    info!("Finished setup");

    Ok(ConfigInfo {
        etna_dir: config.etna_dir.clone(),
        store_path: config.store_path(),
        experiments_path: config.experiments_path(),
        configured: config.configured,
        version: config.version,
    })
}
