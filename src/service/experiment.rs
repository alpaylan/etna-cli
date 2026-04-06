use std::{
    collections::HashMap,
    fs,
    sync::{Arc, Mutex, RwLock},
};

use anyhow::bail;

use crate::{
    driver::run_experiment as driver_run_experiment,
    error_context::Context,
    experiment::{ExperimentMetadata, Test},
    git_driver,
    manager::Manager,
    store::Store,
};

use super::{
    test_utils::{build_invalid_test_message, resolve_test_name},
    types::{
        CreateExperimentOptions, ExperimentInfo, RunExperimentOptions, ServiceResult, TestInfo,
    },
};

/// Create a new experiment
pub fn create_experiment(
    mgr: &mut Manager,
    options: CreateExperimentOptions,
) -> ServiceResult<ExperimentInfo> {
    let CreateExperimentOptions {
        name,
        path,
        overwrite,
        register,
    } = options;

    tracing::trace!("creating new experiment with name '{name}'");

    let path = if let Some(path) = path {
        path
    } else {
        std::env::current_dir().context("Failed to get current directory")?
    };

    let experiment_path = path.join(&name);
    let experiment_exists = mgr.get_experiment(&name);

    // Handle --register and --overwrite logic
    match (
        experiment_path.exists(),
        experiment_exists.as_ref(),
        overwrite,
        register,
    ) {
        (_, _, true, true) => {
            bail!("Cannot use both --register and --overwrite at the same time")
        }
        (true, _, true, false) => {
            tracing::debug!("--overwrite flag is set, removing existing experiment directory");
            fs::remove_dir_all(&experiment_path).with_context(|| {
                format!(
                    "Failed to remove existing experiment directory at '{}'",
                    experiment_path.display()
                )
            })?
        }
        (true, None, false, true) => {
            tracing::debug!("--register flag is set, registering existing experiment");

            let store_path = experiment_path.join("store.jsonl");
            Store::new(store_path.clone())
                .with_context(|| format!("Failed to initialize '{}'", store_path.display()))?;

            let metadata = ExperimentMetadata {
                name: name.clone(),
                path: path.clone(),
                store: store_path,
            };

            mgr.add_experiment(name.clone(), metadata.clone())?;

            tracing::info!(
                "Experiment '{name}' registered successfully at '{}'",
                experiment_path.display()
            );

            return Ok(ExperimentInfo {
                name: metadata.name,
                path: metadata.path.clone(),
                store: metadata.store,
                workloads: vec![],
            });
        }
        (true, Some(_), false, true) => {
            bail!("Experiment already exists: {}", name)
        }
        (true, _, false, false) => {
            bail!(
                "An experiment named '{name}' already exists in '{}'. Use overwrite or register options.",
                fs::canonicalize(&path).unwrap_or(path).display()
            )
        }
        (false, _, true, false) => {
            tracing::warn!(
                "--overwrite flag is set, but the experiment does not exist. Creating experiment as usual."
            )
        }
        (false, _, false, true) => {
            bail!("--register flag is set, but the experiment does not exist")
        }
        (false, _, false, false) => {}
    };

    // Create the experiment directory
    tracing::trace!(
        "creating experiment directory at '{}'",
        experiment_path.display()
    );
    std::fs::create_dir(&experiment_path).with_context(|| {
        format!(
            "Failed to create experiment directory at '{}'",
            experiment_path.display()
        )
    })?;

    // Create the template files
    let template_files = [
        (
            "Collect.py",
            include_str!("../../templates/experimentation/Collect.pyt"),
        ),
        (
            "Query.py",
            include_str!("../../templates/experimentation/Query.pyt"),
        ),
        (
            "Analyze.py",
            include_str!("../../templates/experimentation/Analyze.pyt"),
        ),
        (
            "Visualize.py",
            include_str!("../../templates/experimentation/Visualize.pyt"),
        ),
        (".gitignore", include_str!("../../templates/.gitignoret")),
    ];

    tracing::trace!("creating template files in the experiment directory");
    for (path, content) in template_files.iter() {
        tracing::trace!(
            "Creating template file at '{}/{}'",
            experiment_path.display(),
            path
        );
        let file_path = experiment_path.join(path);
        let parent = file_path.parent().context(format!(
            "Failed to get parent directory for '{}'",
            file_path.display()
        ))?;
        std::fs::create_dir_all(parent)?;
        std::fs::write(&file_path, content).context(format!(
            "Failed to create template file at '{}'",
            file_path.display()
        ))?;
    }

    // Create directories
    let workloads_path = experiment_path.join("workloads");
    tracing::trace!(
        "creating workloads directory at '{}'",
        workloads_path.display()
    );
    std::fs::create_dir(&workloads_path).with_context(|| {
        format!(
            "Failed to create workloads directory at '{}'",
            workloads_path.display()
        )
    })?;

    let scripts_path = experiment_path.join("scripts");
    tracing::trace!("creating scripts directory at '{}'", scripts_path.display());
    std::fs::create_dir(&scripts_path).with_context(|| {
        format!(
            "Failed to create scripts directory at '{}'",
            scripts_path.display()
        )
    })?;

    let tests_path = experiment_path.join("tests");
    tracing::trace!("creating tests directory at '{}'", tests_path.display());
    std::fs::create_dir(&tests_path).with_context(|| {
        format!(
            "Failed to create tests directory at '{}'",
            tests_path.display()
        )
    })?;

    let figures_path = experiment_path.join("figures");
    tracing::trace!("creating figures directory at '{}'", figures_path.display());
    std::fs::create_dir(&figures_path).with_context(|| {
        format!(
            "Failed to create figures directory at '{}'",
            figures_path.display()
        )
    })?;

    // Initialize git repository
    tracing::trace!(
        "initializing git repository at '{}'",
        experiment_path.display()
    );
    git_driver::initialize_git_repo(
        &experiment_path,
        format!("Automated initialization commit for experiment '{}'", name).as_str(),
    )?;

    let store_path = experiment_path.join("store.jsonl");
    Store::new(store_path.clone())
        .with_context(|| format!("Failed to initialize '{}'", store_path.display()))?;

    let metadata = ExperimentMetadata {
        name: name.clone(),
        path: experiment_path.clone(),
        store: store_path,
    };

    mgr.add_experiment(name.clone(), metadata.clone())?;

    tracing::info!(
        "Experiment '{name}' created successfully at '{}'",
        experiment_path.display()
    );

    Ok(ExperimentInfo {
        name: metadata.name,
        path: metadata.path.clone(),
        store: metadata.store,
        workloads: vec![],
    })
}

/// List all experiments
pub fn list_experiments(mgr: &Manager) -> ServiceResult<Vec<ExperimentInfo>> {
    let experiments = mgr
        .experiments
        .values()
        .map(|exp| ExperimentInfo {
            name: exp.name.clone(),
            path: exp.path.clone(),
            store: exp.store.clone(),
            workloads: exp.workloads(),
        })
        .collect();

    Ok(experiments)
}

/// Get a specific experiment by name
pub fn get_experiment(mgr: &Manager, name: &str) -> ServiceResult<ExperimentInfo> {
    let exp = mgr
        .get_experiment(name)
        .ok_or_else(|| anyhow::anyhow!("Experiment not found: {}", name))?;

    Ok(ExperimentInfo {
        name: exp.name.clone(),
        path: exp.path.clone(),
        store: exp.store.clone(),
        workloads: exp.workloads(),
    })
}

/// Delete an experiment
pub fn delete_experiment(mgr: &mut Manager, name: &str, delete_files: bool) -> ServiceResult<()> {
    let exp = mgr
        .get_experiment(name)
        .ok_or_else(|| anyhow::anyhow!("Experiment not found: {}", name))?;

    if delete_files && exp.path.exists() {
        fs::remove_dir_all(&exp.path).with_context(|| {
            format!(
                "Failed to remove experiment directory at '{}'",
                exp.path.display()
            )
        })?;
    }

    mgr.retain_experiments(|e| e.name != name)?;

    tracing::info!("Experiment '{}' deleted successfully", name);
    Ok(())
}

fn get_tests(tests: &[String], experiment: &ExperimentMetadata) -> anyhow::Result<Vec<Test>> {
    if tests.is_empty() {
        anyhow::bail!("No tests provided. Please specify at least one test to run.");
    }

    let available_tests = list_tests(&experiment.path)?
        .into_iter()
        .map(|t| t.name)
        .collect::<Vec<_>>();

    if available_tests.is_empty() {
        anyhow::bail!(
            "No tests found in '{}'. Add workloads first (for example: `etna workload add <lang> <workload>`).",
            experiment.path.join("tests").display()
        );
    }

    let mut all_tests = Vec::new();
    for test in tests {
        let resolved_name = resolve_test_name(test, &available_tests)
            .with_context(|| build_invalid_test_message(test, &available_tests))?;
        let test_path = experiment
            .path
            .join("tests")
            .join(&resolved_name)
            .with_extension("json");

        let content = std::fs::read_to_string(&test_path)
            .with_context(|| format!("Failed to read test from '{}'", test_path.display()))?;
        let test: Vec<Test> = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse test from '{}'", test_path.display()))?;
        all_tests.extend(test);
    }
    Ok(all_tests)
}

/// Run an experiment (synchronously - for async use job service)
pub fn run_experiment(
    mgr: Manager,
    options: RunExperimentOptions,
    cancel_flag: Option<Arc<RwLock<bool>>>,
) -> ServiceResult<()> {
    let RunExperimentOptions {
        experiment_name,
        tests,
        short_circuit,
        parallel,
        params,
    } = options;

    let experiment = mgr
        .get_experiment(&experiment_name)
        .ok_or_else(|| anyhow::anyhow!("Experiment not found: {}", experiment_name))?;

    tracing::trace!("running experiment with name '{:?}'", experiment.name);

    let mut all_tests =
        get_tests(&tests, &experiment).context("Failed to get tests for the experiment")?;

    let cli_params: HashMap<String, String> = params.into_iter().collect();

    let mut mgr = mgr;
    mgr.set_store_path(experiment.store.clone())?;
    mgr.require_store_mut()?.load_metrics()?;

    git_driver::commit(&experiment.path, "Running experiment")?;

    let mgr = Arc::new(Mutex::new(mgr));

    for test in &mut all_tests {
        // Check cancellation before each test
        if let Some(ref flag) = cancel_flag {
            if *flag.read().unwrap() {
                bail!("Job cancelled");
            }
        }

        tracing::info!("Running test: {}", test);
        for p in cli_params.iter() {
            test.params.as_mut().and_then(|params| {
                params.insert(p.0.clone(), serde_json::Value::String(p.1.clone()))
            });
        }
        driver_run_experiment(
            mgr.clone(),
            test,
            &experiment,
            short_circuit,
            parallel,
            &cli_params,
            cancel_flag.clone(),
        )?;
    }

    Ok(())
}

/// List available tests for an experiment
pub fn list_tests(experiment_path: &std::path::Path) -> ServiceResult<Vec<TestInfo>> {
    let tests_dir = experiment_path.join("tests");

    let entries = match fs::read_dir(&tests_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to read tests directory at '{}': {}",
                tests_dir.display(),
                e
            ))
        }
    };

    let mut tests = Vec::new();

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Some(stem) = path.file_stem() {
                tests.push(TestInfo {
                    name: stem.to_string_lossy().to_string(),
                });
            }
        }
    }

    tests.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(tests)
}

/// Get the content of a specific test file
pub fn get_test_content(
    experiment_path: &std::path::Path,
    test_name: &str,
) -> ServiceResult<Vec<crate::experiment::Test>> {
    let test_path = experiment_path
        .join("tests")
        .join(test_name)
        .with_extension("json");

    if !test_path.exists() {
        bail!("Test not found: {}", test_name);
    }

    let content = fs::read_to_string(&test_path)
        .with_context(|| format!("Failed to read test file at '{}'", test_path.display()))?;

    let tests: Vec<crate::experiment::Test> = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse test file at '{}'", test_path.display()))?;

    Ok(tests)
}

/// Save a test file
pub fn save_test(
    experiment_path: &std::path::Path,
    test_name: &str,
    tests: &[crate::experiment::Test],
) -> ServiceResult<()> {
    let test_path = experiment_path
        .join("tests")
        .join(test_name)
        .with_extension("json");

    // Ensure tests directory exists
    if let Some(parent) = test_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create tests directory at '{}'", parent.display())
        })?;
    }

    let content = serde_json::to_string_pretty(tests).context("Failed to serialize tests")?;

    fs::write(&test_path, content)
        .with_context(|| format!("Failed to write test file at '{}'", test_path.display()))?;

    tracing::info!("Saved test '{}' at '{}'", test_name, test_path.display());
    Ok(())
}

/// Delete a test file
pub fn delete_test(experiment_path: &std::path::Path, test_name: &str) -> ServiceResult<()> {
    let test_path = experiment_path
        .join("tests")
        .join(test_name)
        .with_extension("json");

    if !test_path.exists() {
        bail!("Test not found: {}", test_name);
    }

    fs::remove_file(&test_path)
        .with_context(|| format!("Failed to delete test file at '{}'", test_path.display()))?;

    tracing::info!("Deleted test '{}' at '{}'", test_name, test_path.display());
    Ok(())
}

/// Get experiment by path (from current directory)
pub fn get_experiment_from_current_dir(mgr: &Manager) -> ServiceResult<ExperimentInfo> {
    let current_dir = std::env::current_dir().context("Failed to get current directory")?;

    let experiment = mgr
        .experiments
        .values()
        .find(|exp| current_dir.starts_with(&exp.path))
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No experiment found for current directory: {}",
                current_dir.display()
            )
        })?;

    Ok(ExperimentInfo {
        name: experiment.name.clone(),
        path: experiment.path.clone(),
        store: experiment.store.clone(),
        workloads: experiment.workloads(),
    })
}
