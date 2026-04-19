use std::{collections::HashMap, fs, path::Path, process::Command};

use anyhow::bail;
use serde::Deserialize;

use crate::{
    error_context::Context,
    experiment::{ExperimentMetadata, Test},
    git_driver,
    manager::Manager,
    workload::WorkloadMetadata,
};

use super::types::ServiceResult;

/// Copy language files to the experiment workloads directory
fn copy_language(repo_dir: &Path, workloads_dir: &Path, language: &str) -> anyhow::Result<()> {
    git_driver::pull_via_cli(repo_dir)?;

    for entry in repo_dir.join("workloads").join(language).read_dir()? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            fs::copy(
                &path,
                workloads_dir.join(language).join(
                    path.file_name()
                        .context("Failed to get file name")?
                        .to_str()
                        .context("Failed to convert file name to string")?,
                ),
            )
            .with_context(|| {
                format!(
                    "Failed to copy file '{}' to '{}'",
                    path.display(),
                    workloads_dir
                        .join(language)
                        .join(
                            path.file_name()
                                .context("Failed to get file name")
                                .unwrap_or_default()
                                .to_str()
                                .unwrap_or_default()
                        )
                        .display()
                )
            })?;
        } else if path.is_dir() && !path.join("steps.json").exists() {
            // copy the entire directory
            Command::new("cp")
                .arg("-r")
                .arg(&path)
                .arg(
                    workloads_dir.join(language).join(
                        path.file_name()
                            .context("Failed to get directory name")?
                            .to_str()
                            .context("Failed to convert directory name to string")?,
                    ),
                )
                .status()
                .with_context(|| {
                    format!(
                        "Failed to copy directory '{}' to '{}'",
                        path.display(),
                        workloads_dir
                            .join(language)
                            .join(
                                path.file_name()
                                    .context("Failed to get directory name")
                                    .unwrap_or_default()
                                    .to_str()
                                    .unwrap_or_default()
                            )
                            .display()
                    )
                })?;
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct DocTask {
    property: String,
    #[serde(default)]
    counterexample: String,
}

#[derive(Debug, Deserialize)]
struct DocWorkloadEntry {
    mutations: Vec<String>,
    tasks: Vec<DocTask>,
}

/// Build `Test` entries from `docs/workloads/<workload>.json`.
/// Returns an empty vec when the docs file doesn't exist.
pub(crate) fn tests_from_docs(
    repo_dir: &Path,
    language: &str,
    workload: &str,
    trials: usize,
    timeout: f64,
    mode: crate::experiment::Mode,
) -> anyhow::Result<Vec<Test>> {
    let workload_slug = workload.to_lowercase();
    let docs_path = repo_dir
        .join("docs")
        .join("workloads")
        .join(format!("{workload_slug}.json"));

    if !docs_path.exists() {
        tracing::debug!(
            "No docs workload definition found at '{}', skipping test generation",
            docs_path.display()
        );
        return Ok(vec![]);
    }

    let docs_content = fs::read_to_string(&docs_path).with_context(|| {
        format!(
            "Failed to read workload docs file at '{}'",
            docs_path.display()
        )
    })?;

    if docs_content.trim().is_empty() {
        tracing::debug!(
            "Docs workload file '{}' is empty, skipping test generation",
            docs_path.display()
        );
        return Ok(vec![]);
    }

    let entries: Vec<DocWorkloadEntry> =
        serde_json::from_str(&docs_content).with_context(|| {
            format!(
                "Failed to parse workload docs file at '{}'",
                docs_path.display()
            )
        })?;

    if entries.is_empty() {
        tracing::debug!(
            "Docs workload file '{}' has no entries, skipping test generation",
            docs_path.display()
        );
        return Ok(vec![]);
    }

    let generated = entries
        .into_iter()
        .map(|entry| {
            let tasks = entry
                .tasks
                .into_iter()
                .map(|task| {
                    let mut map = HashMap::new();
                    map.insert("property".to_string(), task.property);
                    if !task.counterexample.is_empty() {
                        map.insert("minimal_counterexample".to_string(), task.counterexample);
                    }
                    map
                })
                .collect();

            Test {
                language: language.to_string(),
                workload: workload.to_string(),
                trials,
                timeout,
                mutations: entry.mutations,
                mode: mode.clone(),
                params: None,
                tasks,
            }
        })
        .collect();

    Ok(generated)
}

fn generate_tests_from_docs(
    repo_dir: &Path,
    experiment: &ExperimentMetadata,
    language: &str,
    workload: &str,
) -> anyhow::Result<()> {
    let generated = tests_from_docs(
        repo_dir,
        language,
        workload,
        10,
        60.0,
        crate::experiment::Mode::Solve,
    )?;

    if generated.is_empty() {
        return Ok(());
    }

    let workload_slug = workload.to_lowercase();
    let language_slug = language.to_lowercase();
    let test_path = experiment
        .path
        .join("tests")
        .join(format!("{workload_slug}-{language_slug}"))
        .with_extension("json");

    if let Some(parent) = test_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create tests directory '{}'", parent.display()))?;
    }

    let content =
        serde_json::to_string_pretty(&generated).context("Failed to serialize generated tests")?;
    fs::write(&test_path, content)
        .with_context(|| format!("Failed to write test file at '{}'", test_path.display()))?;

    tracing::info!(
        "Generated test file '{}' from docs/workloads for workload '{}/{}'",
        test_path.display(),
        language,
        workload
    );

    Ok(())
}

/// Add a workload to an experiment
pub fn add_workload(
    mgr: &Manager,
    experiment: &ExperimentMetadata,
    language: &str,
    workload: &str,
) -> ServiceResult<WorkloadMetadata> {
    tracing::debug!(
        "adding workload '{}/{}' to {:?}",
        language,
        workload,
        experiment.name
    );

    // Check if the workload already exists
    if experiment.has_workload(language, workload) {
        bail!("Workload already exists: {}/{}", language, workload);
    }

    // Get etna directory
    let repo_dir = mgr.config.repo_dir();

    // Get the workload path
    let workload_path = repo_dir.join("workloads").join(language).join(workload);

    // Check if the workload exists, pull from remote if not
    if !workload_path.exists() {
        tracing::warn!(
            "Workload '{}' not found, pulling from remote",
            workload_path.display()
        );
        git_driver::pull_via_cli(&repo_dir)?;
    }

    let dest_path = experiment
        .path
        .join("workloads")
        .join(language)
        .join(workload);

    std::fs::create_dir_all(
        dest_path
            .parent()
            .context("Failed to get parent directory")?,
    )
    .context("Failed to create parent directory")?;

    // Copy the language files
    copy_language(&repo_dir, &experiment.path.join("workloads"), language)?;

    // Copy the workload
    Command::new("cp")
        .arg("-r")
        .arg(&workload_path)
        .arg(&dest_path)
        .status()
        .context(format!(
            "Failed to copy workload at '{}' to '{}'",
            fs::canonicalize(&workload_path)
                .context("Failed to get canonical path")?
                .display(),
            dest_path.display()
        ))?;

    // Generate test definitions from docs/workloads/<workload>.json when available.
    generate_tests_from_docs(&repo_dir, experiment, language, workload)?;

    // Create a commit
    git_driver::commit(
        &experiment.path,
        format!("add workload '{}/{}'", language, workload).as_str(),
    )
    .with_context(|| format!("Failed to commit adding '{language}/{workload}'"))?;

    tracing::info!(
        "Workload '{}/{}' added to experiment '{}'",
        language,
        workload,
        experiment.name
    );

    Ok(WorkloadMetadata {
        name: workload.to_string(),
        language: language.to_string(),
    })
}

/// Remove a workload from an experiment
pub fn remove_workload(
    experiment: &ExperimentMetadata,
    language: &str,
    workload: &str,
) -> ServiceResult<()> {
    // Check if the workload exists
    if !experiment.has_workload(language, workload) {
        bail!("Workload not found: {}/{}", language, workload);
    }

    // Remove the workload from the experiment directory
    let dest_path = experiment
        .path
        .join("workloads")
        .join(language)
        .join(workload);

    fs::remove_dir_all(&dest_path).context(format!(
        "Failed to remove workload at '{}'",
        dest_path.display()
    ))?;

    // Create a commit
    git_driver::commit(
        &experiment.path,
        format!("remove '{language}/{workload}'").as_str(),
    )?;

    tracing::info!(
        "Workload '{}/{}' removed from experiment '{}'",
        language,
        workload,
        experiment.name
    );

    Ok(())
}

/// List workloads in an experiment
pub fn list_workloads(
    experiment: &ExperimentMetadata,
    language_filter: Option<&str>,
) -> ServiceResult<Vec<WorkloadMetadata>> {
    let mut workloads: Vec<WorkloadMetadata> = experiment
        .workloads()
        .into_iter()
        .filter(|wl| {
            language_filter.is_none()
                || language_filter == Some("all")
                || language_filter == Some(&wl.language)
        })
        .collect();

    workloads.sort_by(|a, b| a.language.cmp(&b.language).then(a.name.cmp(&b.name)));

    Ok(workloads)
}
