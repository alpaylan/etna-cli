use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{
    service::{mutations::list_mutations, workload as wl_service},
    workload::{InjectionKind, WorkloadManifest},
};

/// Verify that a workload on disk matches its `etna.toml` and that the
/// manifest is internally consistent. Prints one finding per line and
/// exits non-zero if any check fails. Intended to be run from a
/// pre-commit hook.
pub fn invoke(dir: PathBuf) -> anyhow::Result<()> {
    let dir = if dir == Path::new(".") {
        std::env::current_dir()?
    } else {
        dir.canonicalize().unwrap_or(dir)
    };

    let manifest = WorkloadManifest::read(&dir)?;
    let findings = collect_findings(&manifest, &dir);
    let label = dir.display();

    if findings.is_empty() {
        tracing::info!("workload '{}' at {}: ok", manifest.name, label);
        return Ok(());
    }
    for f in &findings {
        eprintln!("{}: {}", label, f);
    }
    anyhow::bail!("{} finding(s) for workload '{}'", findings.len(), manifest.name);
}

/// Run all checks against `manifest` + on-disk state at `dir`. Returns the
/// collected findings. Empty vec means the workload is clean. Pulled out of
/// `invoke` so integration tests can inspect findings without scraping stderr.
pub fn collect_findings(manifest: &WorkloadManifest, dir: &Path) -> Vec<String> {
    let mut findings: Vec<String> = Vec::new();
    check_variant_names(manifest, &mut findings);
    check_variant_set_matches_marauders(manifest, dir, &mut findings);
    check_witnesses_and_properties(manifest, dir, &mut findings);
    check_patch_files_exist(manifest, dir, &mut findings);
    check_commits_resolve(manifest, dir, &mut findings);
    check_docs_idempotent(manifest, dir, &mut findings);
    check_variant_branches_descend(manifest, dir, &mut findings);
    findings
}

fn check_variant_names(manifest: &WorkloadManifest, out: &mut Vec<String>) {
    let re = regex::Regex::new(r"^[a-z][a-z0-9_]*_[0-9a-f]{7}_[0-9]+$").unwrap();
    for g in &manifest.tasks {
        for m in &g.mutations {
            if !re.is_match(m) {
                out.push(format!(
                    "variant name '{}' does not match ^[a-z][a-z0-9_]*_[0-9a-f]{{7}}_[0-9]+$",
                    m
                ));
            }
        }
    }
}

fn check_variant_set_matches_marauders(
    manifest: &WorkloadManifest,
    dir: &Path,
    out: &mut Vec<String>,
) {
    if manifest.tasks.is_empty() {
        return;
    }

    let on_disk: HashSet<String> = match list_mutations(dir) {
        Ok(files) => files
            .into_iter()
            .flat_map(|f| f.mutations.into_iter().map(|m| m.name))
            .collect(),
        Err(e) => {
            out.push(format!("failed to list on-disk mutations: {}", e));
            return;
        }
    };

    let declared: HashSet<String> = manifest
        .tasks
        .iter()
        .flat_map(|g| g.mutations.iter().cloned())
        .collect();

    for m in declared.difference(&on_disk) {
        out.push(format!(
            "manifest declares mutation '{}' but it is not present in source tree or patches/",
            m
        ));
    }
    for m in on_disk.difference(&declared) {
        out.push(format!(
            "mutation '{}' exists in source tree or patches/ but is not declared in any [[tasks]].mutations",
            m
        ));
    }
}

fn check_witnesses_and_properties(
    manifest: &WorkloadManifest,
    dir: &Path,
    out: &mut Vec<String>,
) {
    let src_text = collect_source_text(dir);
    for g in &manifest.tasks {
        for t in &g.tasks {
            let snake = pascal_to_snake(&t.property);
            let needle = format!("fn property_{}", snake);
            if !src_text.contains(&needle) {
                out.push(format!(
                    "property '{}' (expected fn property_{}) not found under src/ or tests/",
                    t.property, snake
                ));
            }
            for w in &t.witnesses {
                if let crate::workload::Witness::TestFn { test_fn, .. } = w {
                    let needle = format!("fn {}", test_fn);
                    if !src_text.contains(&needle) {
                        out.push(format!(
                            "witness '{}' not found under src/ or tests/",
                            test_fn
                        ));
                    }
                }
            }
        }
    }
}

fn check_patch_files_exist(manifest: &WorkloadManifest, dir: &Path, out: &mut Vec<String>) {
    for g in &manifest.tasks {
        let Some(inj) = &g.injection else { continue };
        if !matches!(inj.kind, InjectionKind::Patch) {
            continue;
        }
        let Some(patch) = &inj.patch else {
            out.push(format!(
                "mutation '{}': injection.kind = patch but no injection.patch set",
                g.mutations.first().map(String::as_str).unwrap_or("?")
            ));
            continue;
        };
        let abs = dir.join(patch);
        if !abs.exists() {
            out.push(format!(
                "mutation '{}': patch file '{}' does not exist",
                g.mutations.first().map(String::as_str).unwrap_or("?"),
                patch
            ));
        }
    }
}

fn check_commits_resolve(manifest: &WorkloadManifest, dir: &Path, out: &mut Vec<String>) {
    let mut shas: Vec<String> = Vec::new();
    if let Some(base) = &manifest.base_commit {
        shas.push(base.clone());
    }
    for g in &manifest.tasks {
        if let Some(src) = &g.source {
            shas.extend(src.commits.iter().cloned());
        }
    }
    for sha in shas {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["cat-file", "-e"])
            .arg(&sha)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !ok {
            out.push(format!(
                "commit '{}' does not resolve in this repo (neither base_commit nor source.commits should reference unknown SHAs)",
                sha
            ));
        }
    }
}

fn check_docs_idempotent(manifest: &WorkloadManifest, dir: &Path, out: &mut Vec<String>) {
    let Some(generated) = wl_service::generate_docs(manifest) else {
        return;
    };
    let bugs_path = dir.join("BUGS.md");
    let tasks_path = dir.join("TASKS.md");
    match std::fs::read_to_string(&bugs_path) {
        Ok(on_disk) => {
            if on_disk != generated.bugs_md {
                out.push(
                    "BUGS.md is out of sync with etna.toml (run `etna workload doc`)".into(),
                );
            }
        }
        Err(_) => out.push("BUGS.md is missing (run `etna workload doc`)".into()),
    }
    match std::fs::read_to_string(&tasks_path) {
        Ok(on_disk) => {
            if on_disk != generated.tasks_md {
                out.push(
                    "TASKS.md is out of sync with etna.toml (run `etna workload doc`)".into(),
                );
            }
        }
        Err(_) => out.push("TASKS.md is missing (run `etna workload doc`)".into()),
    }
}

fn check_variant_branches_descend(
    manifest: &WorkloadManifest,
    dir: &Path,
    out: &mut Vec<String>,
) {
    let Some(base) = &manifest.base_commit else {
        return;
    };
    for g in &manifest.tasks {
        for m in &g.mutations {
            let branch = format!("etna/{}", m);
            let exists = Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["rev-parse", "--verify", "--quiet"])
                .arg(&branch)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);
            if !exists {
                continue;
            }
            let ok = Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["merge-base", "--is-ancestor", base, &branch])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if !ok {
                out.push(format!(
                    "base_commit '{}' is not an ancestor of branch '{}'",
                    base, branch
                ));
            }
        }
    }
}

fn collect_source_text(dir: &Path) -> String {
    let mut buf = String::new();
    walk_rs(dir, &mut buf);
    buf
}

fn walk_rs(root: &Path, buf: &mut String) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if path.is_dir() {
            // Skip build artefacts, dotdirs (.git, .marauders, .hegel, …),
            // and the workload's own patches directory.
            if name.starts_with('.') || name == "target" || name == "patches" {
                continue;
            }
            walk_rs(&path, buf);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                buf.push_str(&text);
                buf.push('\n');
            }
        }
    }
}

fn pascal_to_snake(pascal: &str) -> String {
    let mut out = String::with_capacity(pascal.len() + 4);
    for (i, c) in pascal.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::pascal_to_snake;

    #[test]
    fn pascal_to_snake_basics() {
        assert_eq!(pascal_to_snake("ArrayvecDebugMatchesSlice"), "arrayvec_debug_matches_slice");
        assert_eq!(pascal_to_snake("FindIterPrefilterParity"), "find_iter_prefilter_parity");
        assert_eq!(pascal_to_snake("X"), "x");
    }
}
