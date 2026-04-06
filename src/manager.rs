use std::collections::HashMap;

use crate::{commands, config::EtnaConfig, experiment::ExperimentMetadata, store::Store};
use crate::error_context::Context as _;

pub struct Manager {
    pub experiments: HashMap<String, ExperimentMetadata>,
    pub store: Store,
    pub(crate) config: EtnaConfig,
}

impl Manager {
    pub fn load() -> anyhow::Result<Self> {
        // Get Etna configuration
        let etna_config = EtnaConfig::get_etna_config().context("Failed to get etna config")?;

        // Load the Store
        let store = Store::new(etna_config.store_path()).context("Failed to load the store")?;

        // Load all experiments
        let experiments_json_path = etna_config.experiments_path();
        if !experiments_json_path.exists() {
            tracing::warn!("Experiments tracking file does not exist at '{}', running 'etna config setup' to create it", experiments_json_path.display());
            commands::config::setup::invoke(false)?;
        }
        let experiments = serde_json::from_str::<HashMap<String, ExperimentMetadata>>(
            &std::fs::read_to_string(&experiments_json_path)
                .with_context(|| format!("Failed to read '{}'", experiments_json_path.display()))?,
        )?;

        Ok(Self {
            experiments,
            store,
            config: etna_config,
        })
    }

    pub fn save_experiments(&self) -> anyhow::Result<()> {
        let experiments_json_path = self.config.experiments_path();
        let file = std::fs::File::create(&experiments_json_path)
            .with_context(|| format!("Failed to create '{}'", experiments_json_path.display()))?;
        serde_json::to_writer_pretty(file, &self.experiments)
            .with_context(|| format!("Failed to write'{}'", experiments_json_path.display()))?;
        Ok(())
    }

    pub fn get_experiment(&self, name: &str) -> Option<ExperimentMetadata> {
        let experiment = self.experiments.get(name).cloned();
        if let Some(ref experiment) = experiment {
            tracing::debug!("experiment '{}' is found: {:?}", name, experiment);
        } else {
            tracing::debug!("experiment '{}' is not found in etna", name);
        }
        experiment
    }
    pub fn retain_experiments<F>(&mut self, mut f: F) -> anyhow::Result<()>
    where
        F: FnMut(&ExperimentMetadata) -> bool,
    {
        self.experiments.retain(|_, exp| f(exp));
        self.save_experiments()
    }

    pub fn add_experiment(
        &mut self,
        name: String,
        experiment: ExperimentMetadata,
    ) -> anyhow::Result<()> {
        self.experiments.insert(name, experiment);
        self.save_experiments()
    }
}
