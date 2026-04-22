use std::{collections::HashMap, fmt::Write as _, fs, path::Path};

use anyhow::bail;

use crate::{
    error_context::Context,
    experiment::{ExperimentMetadata, Test},
    git_driver,
    manager::Manager,
    workload::{
        InjectionKind, ManifestTaskGroup, Witness, WorkloadManifest, WorkloadMetadata,
    },
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

/// Parsed `etna.toml` plus the raw contents of any sibling doc files
/// (`README.md`, `BUGS.md`, `TASKS.md`). Missing files are reported as `None`.
/// `patches` carries the raw text of every `.patch` file referenced by
/// `injection.patch` across all task groups, keyed by the path string as it
/// appears in the manifest so the UI can look them up verbatim.
///
/// Serialized directly by both the HTTP handler (`GET /experiments/…/workloads/<wl>`)
/// and the static-site publisher (`etna workload site`). One shape, both sinks.
#[derive(Debug, serde::Serialize)]
pub struct WorkloadDetail {
    pub manifest: WorkloadManifest,
    pub readme_md: Option<String>,
    pub bugs_md: Option<String>,
    pub tasks_md: Option<String>,
    pub patches: HashMap<String, String>,
}

/// Load everything the dashboard needs to render a single workload's detail
/// view: the parsed manifest, any well-known sidecar markdown files, and the
/// contents of every patch file referenced by the manifest.
pub fn get_workload_detail(
    experiment: &ExperimentMetadata,
    workload: &str,
) -> ServiceResult<WorkloadDetail> {
    let dir = experiment.workload_path(workload).ok_or_else(|| {
        anyhow::anyhow!(
            "Workload '{}' not found in experiment '{}'",
            workload,
            experiment.name
        )
    })?;

    let manifest = WorkloadManifest::read(&dir)?;
    let read_optional = |file: &str| -> Option<String> {
        fs::read_to_string(dir.join(file)).ok()
    };

    let mut patches: HashMap<String, String> = HashMap::new();
    for group in &manifest.tasks {
        let Some(injection) = &group.injection else { continue };
        let Some(rel) = &injection.patch else { continue };
        if patches.contains_key(rel) {
            continue;
        }
        match fs::read_to_string(dir.join(rel)) {
            Ok(body) => {
                patches.insert(rel.clone(), body);
            }
            Err(e) => {
                tracing::warn!(
                    "Patch '{}' referenced by workload '{}' could not be read: {}",
                    rel,
                    manifest.name,
                    e
                );
            }
        }
    }

    Ok(WorkloadDetail {
        manifest,
        readme_md: read_optional("README.md"),
        bugs_md: read_optional("BUGS.md"),
        tasks_md: read_optional("TASKS.md"),
        patches,
    })
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

/// Rendered markdown output for a workload's docs.
pub struct DocOutputs {
    pub bugs_md: String,
    pub tasks_md: String,
}

/// Fixed framework list: strategies are an experiment-level concern, so the
/// manifest no longer declares them. Docs always show full ✓ coverage.
const FRAMEWORKS: [&str; 4] = ["proptest", "quickcheck", "crabcheck", "hegel"];

/// Deterministically render `BUGS.md` and `TASKS.md` from a workload manifest.
/// Returns `None` when the manifest has no `[[tasks]]` blocks — generator-style
/// workloads (BST, RBT, STLC) don't carry bug-injection metadata and `doc`
/// should no-op for them.
pub fn generate_docs(manifest: &WorkloadManifest) -> Option<DocOutputs> {
    if manifest.tasks.is_empty() {
        return None;
    }
    Some(DocOutputs {
        bugs_md: render_bugs_md(manifest),
        tasks_md: render_tasks_md(manifest),
    })
}

/// Regenerate BUGS.md and TASKS.md in `<dir>`. No-op when the manifest has
/// no bug-injection data. Existing files are overwritten byte-for-byte.
pub fn write_docs(manifest: &WorkloadManifest, dir: &Path) -> anyhow::Result<bool> {
    let Some(out) = generate_docs(manifest) else {
        tracing::info!(
            "Workload '{}' has no [[tasks]] blocks — skipping doc generation",
            manifest.name
        );
        return Ok(false);
    };
    let bugs = dir.join("BUGS.md");
    let tasks = dir.join("TASKS.md");
    fs::write(&bugs, &out.bugs_md)
        .with_context(|| format!("Failed to write '{}'", bugs.display()))?;
    fs::write(&tasks, &out.tasks_md)
        .with_context(|| format!("Failed to write '{}'", tasks.display()))?;
    Ok(true)
}

/// Task groups sorted by first mutation name. Groups without any mutations
/// sort before groups with mutations (deterministic fallback).
fn sorted_groups(manifest: &WorkloadManifest) -> Vec<&ManifestTaskGroup> {
    let mut groups: Vec<&ManifestTaskGroup> = manifest.tasks.iter().collect();
    groups.sort_by(|a, b| a.mutations.first().cmp(&b.mutations.first()));
    groups
}

fn witness_name(w: &Witness) -> &str {
    match w {
        Witness::Input { input, .. } => input,
        Witness::TestFn { test_fn, .. } => test_fn,
    }
}

fn witness_note(w: &Witness) -> Option<&str> {
    match w {
        Witness::Input { note, .. } => note.as_deref(),
        Witness::TestFn { note, .. } => note.as_deref(),
    }
}

fn injection_kind_str(kind: &InjectionKind) -> &'static str {
    match kind {
        InjectionKind::Marauders => "marauders",
        InjectionKind::Patch => "patch",
    }
}

fn render_bugs_md(manifest: &WorkloadManifest) -> String {
    let mut buf = String::new();
    let groups = sorted_groups(manifest);

    writeln!(buf, "# {} — Injected Bugs", manifest.name).unwrap();
    if let Some(desc) = &manifest.description {
        writeln!(buf).unwrap();
        writeln!(buf, "{}", desc.trim()).unwrap();
    }
    writeln!(buf).unwrap();

    let total_mutations: usize = groups.iter().map(|g| g.mutations.len()).sum();
    writeln!(buf, "Total mutations: {}", total_mutations).unwrap();
    writeln!(buf).unwrap();

    // Bug Index
    writeln!(buf, "## Bug Index").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "| # | Variant | Name | Location | Injection | Fix Commit |").unwrap();
    writeln!(buf, "|---|---------|------|----------|-----------|------------|").unwrap();
    let mut idx = 1;
    for g in &groups {
        let short = g
            .bug
            .as_ref()
            .map(|b| b.short_name.as_str())
            .unwrap_or("—");
        let location = bug_location_cell(g);
        let inj_kind = g
            .injection
            .as_ref()
            .map(|i| injection_kind_str(&i.kind))
            .unwrap_or("—");
        let fix = g
            .source
            .as_ref()
            .and_then(|s| s.commits.first())
            .map(|c| format!("`{}`", c))
            .unwrap_or_else(|| "—".to_string());
        for mutation in &g.mutations {
            writeln!(
                buf,
                "| {} | `{}` | `{}` | {} | `{}` | {} |",
                idx, mutation, short, location, inj_kind, fix
            )
            .unwrap();
            idx += 1;
        }
    }
    writeln!(buf).unwrap();

    // Property Mapping
    writeln!(buf, "## Property Mapping").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "| Variant | Property | Witness(es) |").unwrap();
    writeln!(buf, "|---------|----------|-------------|").unwrap();
    for g in &groups {
        for mutation in &g.mutations {
            for task in &g.tasks {
                let ws = task
                    .witnesses
                    .iter()
                    .map(|w| format!("`{}`", witness_name(w)))
                    .collect::<Vec<_>>()
                    .join(", ");
                let ws = if ws.is_empty() { "—".to_string() } else { ws };
                writeln!(buf, "| `{}` | `{}` | {} |", mutation, task.property, ws).unwrap();
            }
        }
    }
    writeln!(buf).unwrap();

    // Framework Coverage (dedup properties, preserve first-seen order)
    writeln!(buf, "## Framework Coverage").unwrap();
    writeln!(buf).unwrap();
    writeln!(
        buf,
        "| Property | proptest | quickcheck | crabcheck | hegel |"
    )
    .unwrap();
    writeln!(
        buf,
        "|----------|---------:|-----------:|----------:|------:|"
    )
    .unwrap();
    let mut seen: std::collections::HashSet<&str> = Default::default();
    let mut props: Vec<&str> = Vec::new();
    for g in &groups {
        for task in &g.tasks {
            if seen.insert(task.property.as_str()) {
                props.push(task.property.as_str());
            }
        }
    }
    for p in &props {
        writeln!(buf, "| `{}` | ✓ | ✓ | ✓ | ✓ |", p).unwrap();
    }
    writeln!(buf).unwrap();

    // Bug Details
    writeln!(buf, "## Bug Details").unwrap();
    for (section, g) in groups.iter().enumerate() {
        writeln!(buf).unwrap();
        let short_name = g
            .bug
            .as_ref()
            .map(|b| b.short_name.as_str())
            .unwrap_or("unnamed");
        writeln!(buf, "### {}. {}", section + 1, short_name).unwrap();
        writeln!(buf).unwrap();

        let vs = g
            .mutations
            .iter()
            .map(|m| format!("`{}`", m))
            .collect::<Vec<_>>()
            .join(", ");
        if !vs.is_empty() {
            writeln!(buf, "- **Variant**: {}", vs).unwrap();
        }
        if let Some(loc) = bug_location_detail(g) {
            writeln!(buf, "- **Location**: {}", loc).unwrap();
        }
        let props_str = g
            .tasks
            .iter()
            .map(|t| format!("`{}`", t.property))
            .collect::<Vec<_>>()
            .join(", ");
        if !props_str.is_empty() {
            writeln!(buf, "- **Property**: {}", props_str).unwrap();
        }

        let all_wits: Vec<&Witness> =
            g.tasks.iter().flat_map(|t| t.witnesses.iter()).collect();
        if !all_wits.is_empty() {
            writeln!(buf, "- **Witness(es)**:").unwrap();
            for w in all_wits {
                let name = witness_name(w);
                match witness_note(w) {
                    Some(n) => writeln!(buf, "  - `{}` — {}", name, n).unwrap(),
                    None => writeln!(buf, "  - `{}`", name).unwrap(),
                }
            }
        }

        if let Some(src) = &g.source {
            let mut refs: Vec<String> = Vec::new();
            for pr in &src.prs {
                refs.push(format!("[#{}]({}/pull/{})", pr, src.repo, pr));
            }
            for iss in &src.issues {
                refs.push(format!("[#{}]({}/issues/{})", iss, src.repo, iss));
            }
            if let Some(d) = &src.discussion {
                refs.push(format!("[discussion]({})", d));
            }
            if let Some(o) = &src.origin {
                refs.push(o.clone());
            }
            let refs_str = refs.join(", ");
            let subject = src.commit_subjects.first().map(String::as_str).unwrap_or("");
            let header = match (!refs_str.is_empty(), !subject.is_empty()) {
                (true, true) => format!("{} — {}", refs_str, subject),
                (true, false) => refs_str,
                (false, true) => subject.to_string(),
                (false, false) => "—".to_string(),
            };
            writeln!(buf, "- **Source**: {}", header).unwrap();
            for line in src.summary.trim().lines() {
                writeln!(buf, "  > {}", line).unwrap();
            }
            if src.commits.len() == 1 {
                let c = &src.commits[0];
                match src.commit_subjects.first() {
                    Some(s) => writeln!(buf, "- **Fix commit**: `{}` — {}", c, s).unwrap(),
                    None => writeln!(buf, "- **Fix commit**: `{}`", c).unwrap(),
                }
            } else if !src.commits.is_empty() {
                writeln!(buf, "- **Fix commits**:").unwrap();
                for (i, c) in src.commits.iter().enumerate() {
                    match src.commit_subjects.get(i) {
                        Some(s) => writeln!(buf, "  - `{}` — {}", c, s).unwrap(),
                        None => writeln!(buf, "  - `{}`", c).unwrap(),
                    }
                }
            }
        }
        if let Some(b) = &g.bug {
            writeln!(buf, "- **Invariant violated**: {}", b.invariant.trim()).unwrap();
            writeln!(
                buf,
                "- **How the mutation triggers**: {}",
                b.how_triggered.trim()
            )
            .unwrap();
        }
    }

    if !manifest.dropped.is_empty() {
        writeln!(buf).unwrap();
        writeln!(buf, "## Dropped Candidates").unwrap();
        writeln!(buf).unwrap();
        for d in &manifest.dropped {
            match &d.subject {
                Some(s) => writeln!(buf, "- `{}` ({}) — {}", d.commit, s, d.reason).unwrap(),
                None => writeln!(buf, "- `{}` — {}", d.commit, d.reason).unwrap(),
            }
        }
    }

    buf
}

/// Render `file:line` for the Bug Index location cell (plain backticks, no
/// `inside …` qualifier).
fn bug_location_cell(g: &ManifestTaskGroup) -> String {
    let inj = match &g.injection {
        Some(i) => i,
        None => return "—".to_string(),
    };
    if let Some(l) = inj.locations.first() {
        return match l.line {
            Some(ln) => format!("`{}:{}`", l.file, ln),
            None => format!("`{}`", l.file),
        };
    }
    if let Some(p) = &inj.patch {
        return format!("`{}`", p);
    }
    if let Some(f) = inj.files.first() {
        return format!("`{}`", f);
    }
    "—".to_string()
}

/// Render the Bug Details location line (adds an `inside <symbol>` qualifier
/// when the manifest carries one).
fn bug_location_detail(g: &ManifestTaskGroup) -> Option<String> {
    let inj = g.injection.as_ref()?;
    if let Some(l) = inj.locations.first() {
        let base = match l.line {
            Some(ln) => format!("`{}:{}`", l.file, ln),
            None => format!("`{}`", l.file),
        };
        return Some(match &l.symbol {
            Some(sym) => format!("{} (inside `{}`)", base, sym),
            None => base,
        });
    }
    if let Some(p) = &inj.patch {
        return Some(format!("`{}`", p));
    }
    inj.files.first().map(|f| format!("`{}`", f))
}

fn render_tasks_md(manifest: &WorkloadManifest) -> String {
    let mut buf = String::new();
    let groups = sorted_groups(manifest);

    writeln!(buf, "# {} — ETNA Tasks", manifest.name).unwrap();
    writeln!(buf).unwrap();

    let total_tasks: usize = groups
        .iter()
        .map(|g| g.mutations.len() * g.tasks.len() * FRAMEWORKS.len())
        .sum();
    writeln!(buf, "Total tasks: {}", total_tasks).unwrap();
    writeln!(buf).unwrap();

    writeln!(buf, "## Task Index").unwrap();
    writeln!(buf).unwrap();
    writeln!(buf, "| Task | Variant | Framework | Property | Witness |").unwrap();
    writeln!(buf, "|------|---------|-----------|----------|---------|").unwrap();
    let mut id = 1;
    for g in &groups {
        for mutation in &g.mutations {
            for task in &g.tasks {
                let w = task
                    .witnesses
                    .first()
                    .map(|w| format!("`{}`", witness_name(w)))
                    .unwrap_or_else(|| "—".to_string());
                for fw in &FRAMEWORKS {
                    writeln!(
                        buf,
                        "| {:03} | `{}` | {} | `{}` | {} |",
                        id, mutation, fw, task.property, w
                    )
                    .unwrap();
                    id += 1;
                }
            }
        }
    }
    writeln!(buf).unwrap();

    writeln!(buf, "## Witness Catalog").unwrap();
    writeln!(buf).unwrap();
    let mut seen: std::collections::HashSet<String> = Default::default();
    for g in &groups {
        for task in &g.tasks {
            for w in &task.witnesses {
                let name = witness_name(w).to_string();
                if !seen.insert(name.clone()) {
                    continue;
                }
                let note = witness_note(w).unwrap_or("base passes, variant fails");
                writeln!(buf, "- `{}` — {}", name, note).unwrap();
            }
        }
    }

    buf
}
