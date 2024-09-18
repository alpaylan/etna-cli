use std::fs;

use anyhow::Context;

use crate::{
    config::{EtnaConfig, ExperimentConfig},
    experiment::Experiment,
    git_driver,
    store::Store,
};

/// A new experiment is create in the provided path
/// If the path is not provided, the current directory is used
/// The directory structure is as follows:
///
/// path/name/
/// |
/// |-------config.toml
/// |-------Collect.py
/// |-------Query.py
/// |-------Analyze.py
/// |-------Visualize.py
/// |-------workloads/
///         |-------[language-1]/
///                 |-------[workload-1]
///                 |-------[workload-2]
///         |-------...
/// |-------.git
/// |-------.gitignore
/// |-------.venv
///
/// # Arguments
/// * `name` - Name of the new experiment
/// * `path` - Path where the new experiment should be created
///
/// config.toml - Configuration file for the experiment
/// - name: Name of the experiment
/// - description: Description of the experiment
/// - [workloads]: List of workloads to be executed
///     - language: Language of the workload
///     - path: Name of the workload
///
/// Collect.py - A default script to collect data from the workloads
/// Query.py - A default script to query the collected data
/// Analyze.py - A default script to analyze the collected data
/// Visualize.py - A default script to visualize the collected data

pub(crate) fn invoke(
    name: String,
    path: Option<std::path::PathBuf>,
    overwrite: bool,
    description: Option<String>,
) -> anyhow::Result<()> {
    // Create a new directory for the experiment
    // If the path is not provided, use the current directory
    let path = if let Some(path) = path {
        path
    } else {
        std::env::current_dir().context("Failed to get current directory")?
    };

    let experiment_path = path.join(&name);
    if experiment_path.exists() {
        if !overwrite {
            anyhow::bail!(
                "Experiment '{name}' already exists in '{}'",
                fs::canonicalize(path)?.display()
            );
        }
        fs::remove_dir_all(&experiment_path)
            .context("Failed to remove existing experiment directory")?;
    }

    std::fs::create_dir(&experiment_path).context("Failed to create experiment directory")?;

    // Create the config file
    let config_path = experiment_path.join("config.toml");
    let description = description.unwrap_or_else(|| "A description of the experiment".to_string());
    let experiment_config = ExperimentConfig::new(&name, &description, experiment_path);

    std::fs::write(
        &config_path,
        toml::to_string(&experiment_config).context("Failed to serialize configuration")?,
    )
    .context("Failed to create config file")?;

    // Create the default scripts
    let scripts = [
        (
            "Collect.py",
            std::include_str!("../../../templates/experimentation/Collect.pyt"),
        ),
        (
            "Query.py",
            std::include_str!("../../../templates/experimentation/Query.pyt"),
        ),
        (
            "Analyze.py",
            std::include_str!("../../../templates/experimentation/Analyze.pyt"),
        ),
        (
            "Visualize.py",
            std::include_str!("../../../templates/experimentation/Visualize.pyt"),
        ),
    ];

    for (script, content) in scripts.iter() {
        let script_path = experiment_config.path.join(script);
        std::fs::write(&script_path, content).context("Failed to create script file")?;
    }

    // Create the workloads directory
    let workloads_path = experiment_config.path.join("workloads");
    std::fs::create_dir(&workloads_path).context("Failed to create workloads directory")?;

    // Create the .gitignore file
    let gitignore_path = experiment_config.path.join(".gitignore");
    std::fs::write(&gitignore_path, "").context("Failed to create .gitignore file")?;

    // Initialize a git repository
    git_driver::initialize_git_repo(
        &experiment_config.path,
        format!("Automated initialization commit for experiment '{}'", name).as_str(),
    )?;

    // Update the etna store with the current experiment
    let etna_config = EtnaConfig::get_etna_config()?;
    let mut etna_store = Store::load(&etna_config.etna_dir.join("store.json"))
        .context("Could not load the store")?;

    let snapshot = etna_store.take_snapshot(&etna_config.repo_dir, &experiment_config.path)?;

    etna_store.experiments.push(Experiment {
        name,
        description: experiment_config.description,
        path: experiment_config.path,
        snapshot,
    });

    etna_store.save(&etna_config.etna_dir.join("store.json"))?;

    Ok(())
}
