//! `etna workload site` — publish the workload catalog as a browsable static
//! site. Every catalog entry gets fetched (just `etna.toml` + its patches)
//! and written as a JSON payload the webview-ui's static entrypoint can
//! render as a detail page.
//!
//! The output directory is laid out so it can be dropped next to the built
//! webview bundle (`etna-vscode/webview-ui/dist/site.html`):
//!
//! ```text
//! <out>/
//!   data/
//!     catalog.json        — every catalog entry + `has_manifest`
//!     workloads/
//!       <name>.json       — one per entry that parsed OK
//! ```

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error_context::Context as _;
use crate::service::site::{fetch_remote_detail, BlockingHttp, FetchOutcome};
use crate::workload_index::{WorkloadEntry, WorkloadIndex};

/// One row in `data/catalog.json`. Mirrors [`WorkloadEntry`] but adds the
/// extra bit the gallery needs: did we manage to fetch a manifest at all?
#[derive(Debug, Serialize)]
struct CatalogRow<'a> {
    #[serde(flatten)]
    entry: &'a WorkloadEntry,
    /// `true` when `data/workloads/<name>.json` exists.
    has_manifest: bool,
}

/// Entry point wired into the clap tree.
pub fn invoke(out: PathBuf, catalog: Option<PathBuf>) -> anyhow::Result<()> {
    let index = match catalog.as_deref() {
        Some(path) => {
            let body = fs::read_to_string(path)
                .with_context(|| format!("Failed to read catalog '{}'", path.display()))?;
            WorkloadIndex::parse(&body)
                .with_context(|| format!("Failed to parse catalog '{}'", path.display()))?
        }
        None => WorkloadIndex::load()?,
    };

    let workloads_dir = out.join("data").join("workloads");
    fs::create_dir_all(&workloads_dir).with_context(|| {
        format!(
            "Failed to create output directory '{}'",
            workloads_dir.display()
        )
    })?;

    let http = BlockingHttp::new()?;
    let mut succeeded: Vec<String> = Vec::new();
    let mut missing: Vec<String> = Vec::new();
    let mut errored: Vec<(String, String)> = Vec::new();
    let mut has_manifest: HashMap<String, bool> = HashMap::new();

    for entry in &index.entries {
        tracing::info!("fetching {} ({})", entry.name, entry.url);
        match fetch_remote_detail(entry, &http) {
            FetchOutcome::Ok { detail } => {
                let path = workloads_dir.join(format!("{}.json", entry.name));
                let body = serde_json::to_string_pretty(&detail).with_context(|| {
                    format!("Failed to serialize detail for '{}'", entry.name)
                })?;
                fs::write(&path, body)
                    .with_context(|| format!("Failed to write '{}'", path.display()))?;
                has_manifest.insert(entry.name.clone(), true);
                succeeded.push(entry.name.clone());
            }
            FetchOutcome::MissingManifest => {
                tracing::warn!("no etna.toml for '{}' — listing only", entry.name);
                has_manifest.insert(entry.name.clone(), false);
                missing.push(entry.name.clone());
            }
            FetchOutcome::Err(e) => {
                tracing::warn!("'{}' failed: {:#}", entry.name, e);
                has_manifest.insert(entry.name.clone(), false);
                errored.push((entry.name.clone(), format!("{:#}", e)));
            }
        }
    }

    write_catalog_json(&out, &index, &has_manifest)?;

    eprintln!(
        "site: {} ok, {} missing manifest, {} errored, {} total",
        succeeded.len(),
        missing.len(),
        errored.len(),
        index.entries.len()
    );
    if !errored.is_empty() {
        eprintln!("errored entries:");
        for (name, reason) in &errored {
            eprintln!("  {} — {}", name, reason);
        }
    }
    eprintln!("wrote {}", out.join("data").display());
    Ok(())
}

fn write_catalog_json(
    out: &Path,
    index: &WorkloadIndex,
    has_manifest: &HashMap<String, bool>,
) -> anyhow::Result<()> {
    let rows: Vec<CatalogRow> = index
        .entries
        .iter()
        .map(|entry| CatalogRow {
            entry,
            has_manifest: *has_manifest.get(&entry.name).unwrap_or(&false),
        })
        .collect();

    #[derive(Serialize)]
    struct CatalogFile<'a> {
        schema_version: u32,
        generated_at: String,
        entries: Vec<CatalogRow<'a>>,
    }

    let file = CatalogFile {
        schema_version: index.schema_version,
        generated_at: chrono::Utc::now().to_rfc3339(),
        entries: rows,
    };

    let path = out.join("data").join("catalog.json");
    let body = serde_json::to_string_pretty(&file).context("Failed to serialize catalog.json")?;
    fs::write(&path, body)
        .with_context(|| format!("Failed to write '{}'", path.display()))?;
    Ok(())
}
