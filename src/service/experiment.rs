use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
};

use anyhow::bail;

use crate::{
    driver::run_experiment as driver_run_experiment,
    error_context::Context,
    experiment::{ExperimentManifest, ExperimentMetadata, Test},
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

fn resolve_experiment_root(path: Option<std::path::PathBuf>) -> anyhow::Result<std::path::PathBuf> {
    if let Some(path) = path {
        if !path.exists() {
            bail!(
                "Provided path '{}' does not exist",
                path.canonicalize()
                    .unwrap_or_else(|_| path.clone())
                    .display()
            );
        }

        if !path.is_dir() {
            bail!("Provided path '{}' is not a directory", path.display());
        }

        Ok(path)
    } else {
        std::env::current_dir().context("Failed to get current directory")
    }
}

/// Register an existing experiment
pub fn register_experiment(
    mgr: &mut Manager,
    name: Option<String>,
    path: Option<PathBuf>,
) -> ServiceResult<ExperimentInfo> {
    let experiment_path = resolve_experiment_root(path)?;

    let name = name.unwrap_or_else(|| {
        let file_name = experiment_path
            .file_name()
            .expect("Experiment path is canonicalized, should not fail");
        file_name.to_string_lossy().to_string()
    });

    tracing::trace!("registering experiment with name '{name}'");

    if mgr.get_experiment(&name).is_some() {
        bail!("Experiment '{}' is already registered", name);
    }

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
        "Experiment '{name}' registered successfully at '{}'",
        experiment_path.display()
    );

    Ok(ExperimentInfo {
        name: metadata.name.clone(),
        path: metadata.path.clone(),
        store: metadata.store.clone(),
        workloads: metadata.workloads(),
        last_activity: last_commit_time(&metadata.path),
    })
}

/// Unix timestamp (seconds) of the latest git commit that touched `path`,
/// or `None` if the path is not inside a git repo or has no commits.
fn last_commit_time(path: &std::path::Path) -> Option<i64> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["log", "-1", "--format=%ct", "--", "."])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout);
    s.trim().parse::<i64>().ok()
}

/// Create a new experiment
pub fn create_experiment(
    mgr: &mut Manager,
    options: CreateExperimentOptions,
) -> ServiceResult<ExperimentInfo> {
    let CreateExperimentOptions {
        name,
        path,
        overwrite,
    } = options;

    tracing::trace!("creating new experiment with name '{name}'");

    let root_path = resolve_experiment_root(path)?;
    let experiment_path = root_path.join(&name);

    match (experiment_path.exists(), overwrite) {
        (true, true) => {
            tracing::debug!("--overwrite flag is set, removing existing experiment directory");
            fs::remove_dir_all(&experiment_path).with_context(|| {
                format!(
                    "Failed to remove existing experiment directory at '{}'",
                    experiment_path.display()
                )
            })?
        }
        (true, false) => {
            bail!(
                "An experiment named '{name}' already exists in '{}'. Use --overwrite to replace it.",
                fs::canonicalize(&root_path)
                    .unwrap_or_else(|_| root_path.clone())
                    .display()
            )
        }
        (false, true) => {
            tracing::warn!(
                "--overwrite flag is set, but the experiment does not exist. Creating experiment as usual."
            )
        }
        (false, false) => {}
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
    let manifest_body = format!("name = \"{name}\"\n");
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
        ("etna.toml", manifest_body.as_str()),
        (
            ".github/workflows/etna-experiment.yml",
            include_str!("../../templates/experiment/etna-experiment.ymlt"),
        ),
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
        last_activity: last_commit_time(&metadata.path),
    })
}

/// Clone a remote experiment repo into `<parent>/<name>/` and register it.
///
/// Uses `git clone --recurse-submodules` so workloads (which the author
/// published as submodules) come down in one shot. The repo's `etna.toml` is
/// the source of truth for the experiment's name.
pub fn clone_experiment(
    mgr: &mut Manager,
    url: &str,
    reference: Option<&str>,
    parent: Option<PathBuf>,
) -> ServiceResult<ExperimentInfo> {
    let parent_dir = match parent {
        Some(p) => {
            fs::create_dir_all(&p)
                .with_context(|| format!("Failed to create parent directory '{}'", p.display()))?;
            p
        }
        None => std::env::current_dir().context("Failed to get current directory")?,
    };

    let tmp_suffix = format!(".tmp-clone-{}", uuid::Uuid::new_v4());
    let tmp_dir = parent_dir.join(&tmp_suffix);

    let clone_result: anyhow::Result<ExperimentMetadata> = (|| {
        git_driver::git_clone_recursive(url, reference, &tmp_dir)?;

        let manifest = ExperimentManifest::read(&tmp_dir)?;
        if manifest.name.is_empty() {
            bail!("etna.toml at '{}' must declare a non-empty `name`", url);
        }

        if mgr.get_experiment(&manifest.name).is_some() {
            bail!("Experiment '{}' is already registered", manifest.name);
        }

        let dest = parent_dir.join(&manifest.name);
        if dest.exists() {
            bail!(
                "Destination '{}' already exists — refusing to overwrite",
                dest.display()
            );
        }

        let store_path = tmp_dir.join("store.jsonl");
        if !store_path.exists() {
            fs::write(&store_path, "").with_context(|| {
                format!(
                    "Failed to seed empty store.jsonl at '{}'",
                    store_path.display()
                )
            })?;
        }

        fs::rename(&tmp_dir, &dest).with_context(|| {
            format!("Failed to move cloned experiment into '{}'", dest.display())
        })?;

        Ok(ExperimentMetadata {
            name: manifest.name.clone(),
            path: dest.clone(),
            store: dest.join("store.jsonl"),
        })
    })();

    if tmp_dir.exists() {
        let _ = fs::remove_dir_all(&tmp_dir);
    }
    let metadata = clone_result?;

    mgr.add_experiment(metadata.name.clone(), metadata.clone())?;

    tracing::info!(
        "Experiment '{}' cloned from '{}' to '{}'",
        metadata.name,
        url,
        metadata.path.display()
    );

    Ok(ExperimentInfo {
        name: metadata.name.clone(),
        path: metadata.path.clone(),
        store: metadata.store.clone(),
        workloads: metadata.workloads(),
        last_activity: last_commit_time(&metadata.path),
    })
}

/// List all experiments, sorted by most-recent git activity (descending),
/// with alphabetical-by-name as the tiebreaker and experiments without git
/// history sunk to the bottom.
pub fn list_experiments(mgr: &Manager) -> ServiceResult<Vec<ExperimentInfo>> {
    let mut experiments: Vec<ExperimentInfo> = mgr
        .experiments
        .values()
        .map(|exp| ExperimentInfo {
            name: exp.name.clone(),
            path: exp.path.clone(),
            store: exp.store.clone(),
            workloads: exp.workloads(),
            last_activity: last_commit_time(&exp.path),
        })
        .collect();

    experiments.sort_by(|a, b| {
        b.last_activity
            .cmp(&a.last_activity)
            .then_with(|| a.name.cmp(&b.name))
    });

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
        last_activity: last_commit_time(&exp.path),
    })
}

/// Delete an experiment.
///
/// When `delete_files` is set, the experiment directory is *moved into the
/// trash* at `$ETNA_HOME/trash/` rather than permanently removed — see
/// [`crate::service::trash`]. The trash is swept on every call, so entries
/// older than [`crate::service::trash::RETENTION`] are reclaimed at that
/// point (not sooner).
pub fn delete_experiment(mgr: &mut Manager, name: &str, delete_files: bool) -> ServiceResult<()> {
    let exp = mgr
        .get_experiment(name)
        .ok_or_else(|| anyhow::anyhow!("Experiment not found: {}", name))?;

    if delete_files && exp.path.exists() {
        super::trash::move_to_trash(&exp.path).with_context(|| {
            format!(
                "Failed to move experiment directory at '{}' to trash",
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

/// Test names become path segments under `<experiment>/tests/`. HTTP handlers
/// pass them straight from percent-decoded URL segments, so reject anything
/// that could escape that directory (`../`, separators, absolute paths).
fn validate_test_name(test_name: &str) -> ServiceResult<()> {
    if test_name.is_empty()
        || test_name.contains('/')
        || test_name.contains('\\')
        || test_name.contains("..")
    {
        bail!("Invalid test name: {:?}", test_name);
    }
    Ok(())
}

/// Get the content of a specific test file
pub fn get_test_content(
    experiment_path: &std::path::Path,
    test_name: &str,
) -> ServiceResult<Vec<crate::experiment::Test>> {
    validate_test_name(test_name)?;
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
    validate_test_name(test_name)?;
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

/// Create a new test file, populating tasks from the workload's `etna.toml`
/// `[[tasks]]` blocks when available.
pub fn create_test(
    _mgr: &crate::manager::Manager,
    experiment: &crate::experiment::ExperimentMetadata,
    test_name: &str,
    workload: &str,
    trials: usize,
    timeout: f64,
    mode: crate::experiment::Mode,
    mutations: Vec<String>,
) -> ServiceResult<()> {
    validate_test_name(test_name)?;
    let test_path = experiment
        .path
        .join("tests")
        .join(test_name)
        .with_extension("json");

    if test_path.exists() {
        bail!(
            "Test '{}' already exists at '{}'",
            test_name,
            test_path.display()
        );
    }

    let workload_path = experiment.workload_path(workload).ok_or_else(|| {
        anyhow::anyhow!(
            "Workload '{}' not found in experiment '{}'. Add it first with `etna workload add <url>`.",
            workload,
            experiment.name,
        )
    })?;

    let manifest = crate::workload::WorkloadManifest::read(&workload_path)?;
    let mut tests = super::workload::tests_from_manifest(&manifest)?;

    // Override the manifest-seeded defaults with caller-supplied trial/timeout/mode.
    for test in tests.iter_mut() {
        test.trials = trials;
        test.timeout = timeout;
        test.mode = mode.clone();
    }

    if !mutations.is_empty() {
        tests.retain(|t| t.mutations.iter().any(|m| mutations.contains(m)));
    }

    if tests.is_empty() {
        tests.push(crate::experiment::Test {
            workload: workload.to_string(),
            trials,
            timeout,
            mutations,
            mode,
            params: None,
            tasks: vec![],
        });
    }

    save_test(&experiment.path, test_name, &tests)?;

    Ok(())
}

/// Delete a test file
pub fn delete_test(experiment_path: &std::path::Path, test_name: &str) -> ServiceResult<()> {
    validate_test_name(test_name)?;
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
        last_activity: last_commit_time(&experiment.path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: test names arrive percent-decoded from URL segments and
    /// were joined into filesystem paths unchecked, allowing `../` traversal
    /// out of the experiment's tests/ directory for read, write, and delete.
    #[test]
    fn test_names_with_traversal_are_rejected() {
        for bad in ["../evil", "..", "a/b", "a\\b", "/etc/passwd", ""] {
            assert!(validate_test_name(bad).is_err(), "accepted {bad:?}");
        }
        for good in ["t", "rust-3way", "bst_haskell.v2"] {
            assert!(validate_test_name(good).is_ok(), "rejected {good:?}");
        }
    }

    #[test]
    fn save_test_refuses_to_write_outside_tests_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let exp = tmp.path().join("exp");
        std::fs::create_dir_all(exp.join("tests")).unwrap();

        let err = save_test(&exp, "../escaped", &[]).unwrap_err();
        assert!(format!("{err:?}").contains("Invalid test name"));
        assert!(
            !tmp.path().join("escaped.json").exists(),
            "traversal escaped the tests directory"
        );
    }
}
