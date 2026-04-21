//! Workload catalog — the "phone book" for `etna workload add <name>`.
//!
//! Sources, in order of precedence:
//! 1. `~/.etna/workloads-index.json` — cache written by `etna workload update`
//!    (or the first successful `fetch()` call).
//! 2. The bundled snapshot baked in via `include_str!` at compile time.
//!
//! `load()` is the common read path — it is pure filesystem, never touches the
//! network, and never fails for transient reasons. `fetch()` is the explicit
//! refresh path (called by `etna workload update` and the
//! `POST /api/v1/workloads/index/refresh` route) that talks HTTP and rewrites
//! the cache.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::EtnaConfig;
use crate::error_context::Context as _;

/// Default URL the CLI pulls from when no override is set. Points at this
/// repo's `main` branch — update the bundled snapshot and push to update
/// everyone.
pub const DEFAULT_INDEX_URL: &str =
    "https://raw.githubusercontent.com/alpaylan/etna2/main/docs/workloads/index.json";

/// Env override for the canonical URL. Useful in tests (point at a `file://`
/// fixture) and for users running a private catalog.
pub const INDEX_URL_ENV: &str = "ETNA_WORKLOAD_INDEX_URL";

/// Bundled snapshot of `docs/workloads/index.json` at build time. Used when
/// the cache is missing or unreadable so `etna workload list --kind available`
/// always has something to show.
const BUNDLED_INDEX: &str = include_str!("../docs/workloads/index.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkloadIndex {
    pub schema_version: u32,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub entries: Vec<WorkloadEntry>,
    #[serde(default)]
    pub shared_libs: Vec<SharedLib>,
    #[serde(default)]
    pub notes: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkloadEntry {
    pub name: String,
    pub url: String,
    pub language: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default_ref: Option<String>,
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_status() -> String {
    "stable".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SharedLib {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub consumed_by: Option<String>,
    #[serde(default)]
    pub submodule_path: Option<String>,
}

impl WorkloadIndex {
    /// Parse bytes as an index. Bubbles up serde errors with context.
    pub fn parse(body: &str) -> anyhow::Result<Self> {
        serde_json::from_str(body).context("Failed to parse workload index JSON")
    }

    /// Bundled snapshot — infallible as long as the repo ships a valid file
    /// (compile-time guarantee via `include_str!`). We still return Result so
    /// a corrupted bundle surfaces instead of panicking.
    pub fn bundled() -> anyhow::Result<Self> {
        Self::parse(BUNDLED_INDEX)
    }

    /// Load from cache if present and parseable; otherwise fall back to the
    /// bundled snapshot. Never returns an error for a missing cache — that's
    /// the normal first-run state.
    pub fn load() -> anyhow::Result<Self> {
        let cache_path = match cache_path() {
            Some(p) => p,
            None => return Self::bundled(),
        };
        match std::fs::read_to_string(&cache_path) {
            Ok(body) => match Self::parse(&body) {
                Ok(idx) => Ok(idx),
                Err(e) => {
                    tracing::warn!(
                        "Cached workload index at '{}' is unreadable ({:#}); using bundled snapshot.",
                        cache_path.display(),
                        e
                    );
                    Self::bundled()
                }
            },
            Err(_) => Self::bundled(),
        }
    }

    /// Fetch the canonical index over HTTP, write it to the cache, and return
    /// it. Honors `ETNA_WORKLOAD_INDEX_URL` for tests and private overrides.
    pub fn fetch() -> anyhow::Result<Self> {
        let url = index_url();
        tracing::info!("Refreshing workload index from '{}'", url);

        let body = if let Some(path) = url.strip_prefix("file://") {
            std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read index from file URL '{}'", url))?
        } else {
            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .context("Failed to build HTTP client for index fetch")?;
            client
                .get(&url)
                .send()
                .with_context(|| format!("Failed to GET '{}'", url))?
                .error_for_status()
                .with_context(|| format!("Non-success status from '{}'", url))?
                .text()
                .context("Failed to read index response body")?
        };

        let index = Self::parse(&body)?;

        let cache_path = cache_path()
            .ok_or_else(|| anyhow::anyhow!("No $ETNA_HOME or home directory to cache into"))?;
        write_cache(&cache_path, &body)?;

        Ok(index)
    }

    /// Look up an entry by name. Case-sensitive — names are stable identifiers.
    pub fn resolve(&self, name: &str) -> Option<&WorkloadEntry> {
        self.entries.iter().find(|e| e.name == name)
    }
}

fn cache_path() -> Option<std::path::PathBuf> {
    EtnaConfig::get_etna_config()
        .map(|cfg| cfg.workload_index_path())
        .ok()
        .or_else(|| {
            EtnaConfig::get_etna_dir()
                .ok()
                .map(|dir| dir.join("workloads-index.json"))
        })
}

fn write_cache(path: &Path, body: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create cache directory '{}'", parent.display())
        })?;
    }
    std::fs::write(path, body)
        .with_context(|| format!("Failed to write '{}'", path.display()))?;
    Ok(())
}

fn index_url() -> String {
    if let Ok(url) = std::env::var(INDEX_URL_ENV) {
        if !url.is_empty() {
            return url;
        }
    }
    if let Ok(cfg) = EtnaConfig::get_etna_config() {
        if let Some(url) = cfg.workload_index_url {
            if !url.is_empty() {
                return url;
            }
        }
    }
    DEFAULT_INDEX_URL.to_string()
}

/// Classify an `add` argument: is it a URL we should clone directly, or a
/// bare name to look up in the index? The heuristic is deliberately simple —
/// anything containing `://` or matching `user@host:path` is treated as a URL;
/// filesystem paths (tests use these) also parse as URLs once absolute.
pub fn looks_like_url(spec: &str) -> bool {
    if spec.contains("://") {
        return true;
    }
    // `git@github.com:org/repo.git`
    if let Some(at_idx) = spec.find('@') {
        if spec[at_idx..].contains(':') {
            return true;
        }
    }
    // Absolute filesystem paths — test fixtures pass these as URLs to
    // `git submodule add`.
    if spec.starts_with('/') {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_parses() {
        let idx = WorkloadIndex::bundled().expect("bundled snapshot must parse");
        assert_eq!(idx.schema_version, 1);
        assert!(!idx.entries.is_empty(), "bundled index should not be empty");
        assert!(
            idx.resolve("bst-haskell").is_some(),
            "bst-haskell should be in the bundled index"
        );
    }

    #[test]
    fn resolve_returns_entry() {
        let idx = WorkloadIndex::bundled().unwrap();
        let entry = idx.resolve("bst-rust").expect("bst-rust should resolve");
        assert_eq!(entry.language, "Rust");
        assert!(entry.url.contains("etna-rust-bst"));
    }

    #[test]
    fn resolve_unknown_is_none() {
        let idx = WorkloadIndex::bundled().unwrap();
        assert!(idx.resolve("does-not-exist").is_none());
    }

    #[test]
    fn url_heuristic() {
        assert!(looks_like_url("https://github.com/x/y"));
        assert!(looks_like_url("http://example.com/repo"));
        assert!(looks_like_url("git@github.com:org/repo.git"));
        assert!(looks_like_url("file:///tmp/repo"));
        assert!(looks_like_url("/abs/path/to/repo"));

        assert!(!looks_like_url("bst-haskell"));
        assert!(!looks_like_url("my-workload"));
        assert!(!looks_like_url("simple_name"));
    }

    #[test]
    fn entry_defaults() {
        let body = r#"{
            "schema_version": 1,
            "entries": [
                { "name": "x", "url": "https://example.com/x", "language": "Rust" }
            ]
        }"#;
        let idx = WorkloadIndex::parse(body).unwrap();
        let e = &idx.entries[0];
        assert_eq!(e.status, "stable");
        assert!(e.tags.is_empty());
        assert_eq!(e.default_ref, None);
        assert_eq!(e.description, None);
    }
}
