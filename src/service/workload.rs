use std::{collections::HashMap, fs, path::Path};

use anyhow::bail;

use crate::{
    error_context::Context,
    experiment::{ExperimentMetadata, Test},
    git_driver,
    manager::Manager,
    workload::{WorkloadManifest, WorkloadMetadata},
    workload_index::{looks_like_url, WorkloadEntry, WorkloadIndex},
};

use super::types::ServiceResult;

/// Resolved `add` target: a git URL plus an optional ref. `name` is the
/// catalog name when the caller passed one, else `None`.
pub struct ResolvedSpec {
    pub url: String,
    pub reference: Option<String>,
    pub name: Option<String>,
}

/// Turn a user-supplied spec (either a git URL or a catalog name) into the
/// URL+ref the rest of `add_workload` expects. When the caller supplied an
/// explicit `--ref`, it wins over the entry's `default_ref`.
pub fn resolve_spec(spec: &str, reference: Option<&str>) -> ServiceResult<ResolvedSpec> {
    if looks_like_url(spec) {
        return Ok(ResolvedSpec {
            url: spec.to_string(),
            reference: reference.map(str::to_string),
            name: None,
        });
    }

    let index = WorkloadIndex::load()?;
    let entry = index.resolve(spec).ok_or_else(|| {
        anyhow::anyhow!(
            "Workload '{}' not found in the catalog. Run `etna workload list --kind available` to see what's there, or pass a full git URL.",
            spec
        )
    })?;

    let reference = reference
        .map(str::to_string)
        .or_else(|| entry.default_ref.clone());

    Ok(ResolvedSpec {
        url: entry.url.clone(),
        reference,
        name: Some(entry.name.clone()),
    })
}

/// Default trial count seeded into `tests/<name>.json` when adding a workload.
/// Users can edit the file afterwards; this is just the first-run baseline.
const DEFAULT_TRIALS: usize = 10;
/// Default per-trial timeout (seconds).
const DEFAULT_TIMEOUT: f64 = 60.0;

/// Build `Test` entries from a workload's `etna.toml` manifest. Each
/// `[[tasks]]` block becomes one `Test` keyed by its mutation subset. Returns
/// an empty vec when the manifest has no `[[tasks]]` blocks.
pub(crate) fn tests_from_manifest(manifest: &WorkloadManifest) -> Vec<Test> {
    manifest
        .tasks
        .iter()
        .map(|group| Test {
            workload: manifest.name.clone(),
            trials: DEFAULT_TRIALS,
            timeout: DEFAULT_TIMEOUT,
            mutations: group.mutations.clone(),
            mode: crate::experiment::Mode::Solve,
            params: None,
            tasks: group
                .tasks
                .iter()
                .map(|task| {
                    let mut map = HashMap::new();
                    map.insert(
                        "property".to_string(),
                        serde_json::Value::String(task.property.clone()),
                    );
                    if !task.witnesses.is_empty() {
                        map.insert(
                            "witnesses".to_string(),
                            serde_json::to_value(&task.witnesses)
                                .unwrap_or(serde_json::Value::Null),
                        );
                    }
                    map
                })
                .collect(),
        })
        .collect()
}

fn seed_tests_file(
    experiment: &ExperimentMetadata,
    manifest: &WorkloadManifest,
) -> anyhow::Result<()> {
    let generated = tests_from_manifest(manifest);
    if generated.is_empty() {
        return Ok(());
    }

    let test_path = experiment
        .path
        .join("tests")
        .join(&manifest.name)
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
        "Seeded '{}' from etna.toml for workload '{}'",
        test_path.display(),
        manifest.name
    );

    Ok(())
}

/// Add a remote workload to an experiment as a git submodule.
///
/// Two-phase:
/// 1. Shallow-clone `<url>` to a temp dir just to read `etna.toml` so we know
///    where the workload should live (`workloads/<name>/`). The temp clone is
///    then removed.
/// 2. Run `git -C <experiment> submodule add [--branch <ref>] <url>
///    workloads/<name>`. This performs the real clone, writes `.gitmodules`,
///    and stages both the submodule gitlink and `.gitmodules` entry so the
///    subsequent commit captures the workload at a pinned SHA — ready to be
///    pushed as part of an experiment repo.
///
/// The `.git` directory inside the added workload is still the provenance
/// record (URL / SHA / ref recoverable via `git -C <wl> …`).
pub fn add_workload(
    _mgr: &Manager,
    experiment: &ExperimentMetadata,
    spec: &str,
    reference: Option<&str>,
) -> ServiceResult<WorkloadMetadata> {
    let resolved = resolve_spec(spec, reference)?;
    let url = resolved.url.as_str();
    let reference = resolved.reference.as_deref();
    tracing::debug!(
        "adding workload from '{}' (ref: {:?}) to '{}'",
        url,
        reference,
        experiment.name
    );

    let workloads_dir = experiment.path.join("workloads");
    fs::create_dir_all(&workloads_dir).with_context(|| {
        format!(
            "Failed to create workloads directory '{}'",
            workloads_dir.display()
        )
    })?;

    let tmp_suffix = format!(".tmp-discover-{}", uuid::Uuid::new_v4());
    let tmp_dir = workloads_dir.join(&tmp_suffix);

    let manifest_result: anyhow::Result<WorkloadManifest> = (|| {
        git_driver::git_clone(url, reference, &tmp_dir)?;

        let manifest = WorkloadManifest::read(&tmp_dir)?;
        if manifest.name.is_empty() {
            bail!("etna.toml at '{}' must declare a non-empty `name`", url);
        }
        if !tmp_dir.join("steps.json").exists() {
            bail!(
                "Workload repo '{}' is missing a `steps.json` at its root",
                url
            );
        }
        Ok(manifest)
    })();

    if tmp_dir.exists() {
        let _ = fs::remove_dir_all(&tmp_dir);
    }
    let manifest = manifest_result?;

    if experiment.workload_path(&manifest.name).is_some() {
        bail!(
            "Workload '{}' is already registered in experiment '{}'",
            manifest.name,
            experiment.name
        );
    }

    let dest = workloads_dir.join(&manifest.name);
    if dest.exists() {
        bail!(
            "Workload '{}' already exists in experiment '{}'",
            manifest.name,
            experiment.name
        );
    }

    // Phase 2: real add as a submodule, pinned to whatever ref we were given.
    let submodule_path = Path::new("workloads").join(&manifest.name);
    git_driver::git_submodule_add(&experiment.path, url, reference, &submodule_path)
        .with_context(|| {
            format!(
                "Failed to add workload '{}' as submodule in experiment '{}'",
                manifest.name, experiment.name
            )
        })?;

    // Seed tests from the manifest's `[[tasks]]` blocks (no-op if absent).
    // Failure is logged but non-fatal — the workload is still registered.
    if let Err(e) = seed_tests_file(experiment, &manifest) {
        tracing::warn!(
            "Failed to seed test file for '{}': {:#}",
            manifest.name,
            e
        );
    }

    let wl_meta = WorkloadMetadata {
        name: manifest.name,
    };

    git_driver::commit(
        &experiment.path,
        &format!("add workload '{}' from {}", wl_meta.name, url),
    )
    .with_context(|| format!("Failed to commit adding '{}'", wl_meta.name))?;

    tracing::info!(
        "Workload '{}' added to experiment '{}'",
        wl_meta.name,
        experiment.name
    );

    Ok(wl_meta)
}

/// Remove a workload from an experiment by name.
///
/// Workloads are tracked as git submodules, so a clean removal has to
/// `git submodule deinit` and `git rm` the submodule — a plain `rm -rf`
/// would leave `.gitmodules` and the gitlink in a dirty state.
pub fn remove_workload(
    experiment: &ExperimentMetadata,
    workload: &str,
) -> ServiceResult<()> {
    let dest = experiment.workload_path(workload).ok_or_else(|| {
        anyhow::anyhow!(
            "Workload '{}' not found in experiment '{}'",
            workload,
            experiment.name
        )
    })?;

    let submodule_path = dest
        .strip_prefix(&experiment.path)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| Path::new("workloads").join(workload));

    let submodule_result = git_driver::git_submodule_remove(&experiment.path, &submodule_path);

    // Fall back to a plain rm if the submodule machinery balked (e.g. the
    // workload was added before submodule support — legacy layouts). Git also
    // leaves `<repo>/.git/modules/<path>` behind on deinit+rm; clean that too.
    if submodule_result.is_err() && dest.exists() {
        fs::remove_dir_all(&dest).context(format!(
            "Failed to remove workload at '{}'",
            dest.display()
        ))?;
    }
    let modules_leftover = experiment
        .path
        .join(".git")
        .join("modules")
        .join(&submodule_path);
    if modules_leftover.exists() {
        let _ = fs::remove_dir_all(&modules_leftover);
    }

    git_driver::commit(
        &experiment.path,
        &format!("remove workload '{}'", workload),
    )?;

    tracing::info!(
        "Workload '{}' removed from experiment '{}'",
        workload,
        experiment.name
    );

    Ok(())
}

/// List workloads in an experiment.
pub fn list_workloads(experiment: &ExperimentMetadata) -> ServiceResult<Vec<WorkloadMetadata>> {
    Ok(experiment.workloads())
}

/// List workloads available to add, sourced from the workload catalog
/// (`~/.etna/workloads-index.json` when present, otherwise the bundled
/// snapshot). Never touches the network — call `update_index()` to refresh.
pub fn list_available_workloads() -> ServiceResult<Vec<WorkloadEntry>> {
    Ok(WorkloadIndex::load()?.entries)
}

/// Force-refresh the cached workload index from the canonical URL. Returns
/// the fresh index so callers can report how many entries it has.
pub fn update_index() -> ServiceResult<WorkloadIndex> {
    WorkloadIndex::fetch()
}
