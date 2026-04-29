//! Helpers for the `etna workload site` publisher.
//!
//! The publisher reads the catalog (see [`crate::workload_index`]) and, for
//! each entry, pulls just the bits it needs — `etna.toml` and the patch files
//! each group references — over HTTP. No submodule clones, no on-disk state;
//! the whole thing runs against remote repos.
//!
//! The output of [`fetch_remote_detail`] is a [`WorkloadDetail`] identical in
//! shape to what the HTTP server hands to the VS Code webview. Same JSON, same
//! React rendering — the static site can reuse the extension's
//! `WorkloadDetail.tsx` verbatim.
//!
//! GitHub is the only host we parse out in v1 because every catalog entry
//! points there. Non-GitHub URLs surface a typed error so the publisher can
//! skip-with-warning rather than aborting the whole run.

use std::collections::HashMap;

use crate::error_context::Context as _;
use crate::service::workload::{generate_docs, WorkloadDetail};
use crate::workload::WorkloadManifest;
use crate::workload_index::WorkloadEntry;

/// Default branches to try, in order, when a catalog entry doesn't pin a
/// `default_ref`. The crate-fork workloads (bstr-etna, memchr-etna, etc.)
/// inherit `master` from their upstream; newer workloads use `main`. Try
/// both rather than requiring every entry to spell out `default_ref`.
const FALLBACK_REFS: &[&str] = &["main", "master"];

/// Return the ordered list of refs to try for a catalog entry. When the
/// entry pins `default_ref`, that's the only candidate. Otherwise the
/// fallback list wins.
fn candidate_refs(entry_ref: Option<&str>) -> Vec<&str> {
    match entry_ref {
        Some(r) => vec![r],
        None => FALLBACK_REFS.to_vec(),
    }
}

/// Derive the raw-content base URL for a GitHub repo URL + a specific ref.
/// The returned string always ends with a slash so callers can concatenate
/// a relative path onto it without juggling separators.
///
/// Supports the two URL forms we see in the wild:
/// - `https://github.com/<owner>/<repo>` (with or without a `.git` suffix or
///   trailing slash)
/// - `git@github.com:<owner>/<repo>(.git)` — less common in the catalog but
///   cheap to handle.
///
/// Returns `Err` for non-GitHub URLs; callers should treat that as a
/// skip-with-warning, not a fatal error.
pub fn github_raw_base(url: &str, reference: &str) -> anyhow::Result<String> {
    let (owner, repo) = parse_github_slug(url)?;
    Ok(format!(
        "https://raw.githubusercontent.com/{}/{}/{}/",
        owner, repo, reference
    ))
}

fn parse_github_slug(url: &str) -> anyhow::Result<(String, String)> {
    // Normalize to "<owner>/<repo>" regardless of scheme.
    let rest = if let Some(r) = url.strip_prefix("https://github.com/") {
        r.to_string()
    } else if let Some(r) = url.strip_prefix("http://github.com/") {
        r.to_string()
    } else if let Some(r) = url.strip_prefix("git@github.com:") {
        r.to_string()
    } else if let Some(r) = url.strip_prefix("ssh://git@github.com/") {
        r.to_string()
    } else {
        anyhow::bail!(
            "URL '{}' is not recognised as a GitHub repo — the site publisher \
             only knows how to fetch from github.com in v1",
            url
        );
    };

    // Strip trailing `.git` / `/` so the owner/repo split is clean.
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = rest.splitn(3, '/');
    let owner = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("GitHub URL '{}' has no owner segment", url))?;
    let repo = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("GitHub URL '{}' has no repo segment", url))?;
    Ok((owner.to_string(), repo.to_string()))
}

/// HTTP transport the publisher uses. Factored out so we can swap in a fake
/// in tests without bringing up an HTTP server.
pub trait HttpFetcher {
    /// Return the body of the resource or `Ok(None)` if the server replied 404.
    /// Every other error (timeout, 5xx, parse failure) bubbles up.
    fn fetch_text(&self, url: &str) -> anyhow::Result<Option<String>>;
}

/// Real HTTP client used by the CLI. Blocking because the publisher is a
/// one-shot batch — no async runtime in play.
pub struct BlockingHttp {
    client: reqwest::blocking::Client,
}

impl BlockingHttp {
    pub fn new() -> anyhow::Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent(concat!("etna-site/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("Failed to build HTTP client for site publisher")?;
        Ok(Self { client })
    }
}

impl HttpFetcher for BlockingHttp {
    fn fetch_text(&self, url: &str) -> anyhow::Result<Option<String>> {
        let res = self
            .client
            .get(url)
            .send()
            .with_context(|| format!("Failed to GET '{}'", url))?;
        if res.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let res = res
            .error_for_status()
            .with_context(|| format!("Non-success status from '{}'", url))?;
        let body = res
            .text()
            .with_context(|| format!("Failed to read body of '{}'", url))?;
        Ok(Some(body))
    }
}

/// Outcome of attempting to publish a single catalog entry. Carries just
/// enough context for a useful one-line summary at the end of the run.
#[derive(Debug)]
pub enum FetchOutcome {
    /// Successfully fetched and parsed. Patch-fetch warnings (if any) were
    /// logged and the missing entries are absent from `detail.patches`.
    Ok { detail: WorkloadDetail },
    /// Manifest was absent — usually an old catalog entry pointing at a repo
    /// that predates the `etna.toml` convention.
    MissingManifest,
    /// URL doesn't parse as a supported GitHub URL, or some other hard error
    /// (network down, bad TOML, etc.). Caller decides whether to log or abort.
    Err(anyhow::Error),
}

/// Pull the bits needed to render a static detail page for one catalog entry.
///
/// Strategy:
/// 1. Fetch `etna.toml`. A 404 returns `MissingManifest` (non-fatal).
/// 2. Parse it with the canonical [`WorkloadManifest`] deserializer.
/// 3. For each deduped `injection.patch`, fetch the referenced file. Misses
///    log a warning and drop out of the returned map — the UI already renders
///    "patch not available" for them.
/// 4. Best-effort fetch `README.md` — optional.
/// 5. Regenerate `BUGS.md` / `TASKS.md` locally from the parsed manifest via
///    `generate_docs`. No second round-trip needed.
pub fn fetch_remote_detail<H: HttpFetcher>(
    entry: &WorkloadEntry,
    http: &H,
) -> FetchOutcome {
    // Try each candidate ref until one serves etna.toml. Once we find a
    // ref that works, all subsequent fetches (patches, README) reuse it
    // so we stay inside one consistent repo snapshot.
    let refs = candidate_refs(entry.default_ref.as_deref());
    let mut base: Option<String> = None;
    let mut manifest_body: Option<String> = None;
    for r in &refs {
        let candidate_base = match github_raw_base(&entry.url, r) {
            Ok(b) => b,
            Err(e) => return FetchOutcome::Err(e),
        };
        match http.fetch_text(&format!("{}etna.toml", candidate_base)) {
            Ok(Some(body)) => {
                base = Some(candidate_base);
                manifest_body = Some(body);
                break;
            }
            Ok(None) => continue,
            Err(e) => return FetchOutcome::Err(e),
        }
    }
    let (base, manifest_body) = match (base, manifest_body) {
        (Some(b), Some(m)) => (b, m),
        _ => return FetchOutcome::MissingManifest,
    };

    let manifest: WorkloadManifest = match toml::from_str(&manifest_body) {
        Ok(m) => m,
        Err(e) => {
            return FetchOutcome::Err(anyhow::anyhow!(
                "Failed to parse etna.toml for '{}': {}",
                entry.name,
                e
            ));
        }
    };

    let mut patches: HashMap<String, String> = HashMap::new();
    for group in &manifest.tasks {
        let Some(injection) = &group.injection else { continue };
        let Some(rel) = &injection.patch else { continue };
        if patches.contains_key(rel) {
            continue;
        }
        let url = format!("{}{}", base, rel);
        match http.fetch_text(&url) {
            Ok(Some(body)) => {
                patches.insert(rel.clone(), body);
            }
            Ok(None) => {
                tracing::warn!(
                    "Workload '{}' references patch '{}' but it is 404 at '{}'",
                    entry.name,
                    rel,
                    url
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Workload '{}' patch '{}' failed: {:#}",
                    entry.name,
                    rel,
                    e
                );
            }
        }
    }

    // README is optional — don't fail the whole entry on 404 or network flake.
    let readme_md = http
        .fetch_text(&format!("{}README.md", base))
        .ok()
        .flatten();

    // Deterministically regenerate the docs from the manifest. This keeps the
    // published site consistent even when the on-repo BUGS.md/TASKS.md is stale.
    let (bugs_md, tasks_md) = match generate_docs(&manifest) {
        Some(out) => (Some(out.bugs_md), Some(out.tasks_md)),
        None => (None, None),
    };

    FetchOutcome::Ok {
        detail: WorkloadDetail {
            manifest,
            readme_md,
            bugs_md,
            tasks_md,
            patches,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_url() {
        let (o, r) = parse_github_slug("https://github.com/foo/bar").unwrap();
        assert_eq!(o, "foo");
        assert_eq!(r, "bar");
    }

    #[test]
    fn parses_https_url_with_trailing_slash_and_git_suffix() {
        let (o, r) = parse_github_slug("https://github.com/foo/bar.git/").unwrap();
        assert_eq!(o, "foo");
        assert_eq!(r, "bar");
    }

    #[test]
    fn parses_git_ssh_url() {
        let (o, r) = parse_github_slug("git@github.com:foo/bar.git").unwrap();
        assert_eq!(o, "foo");
        assert_eq!(r, "bar");
    }

    #[test]
    fn rejects_non_github_url() {
        let err = parse_github_slug("https://gitlab.com/foo/bar").unwrap_err();
        assert!(err.to_string().contains("github.com"));
    }

    #[test]
    fn raw_base_with_supplied_ref() {
        let b = github_raw_base("https://github.com/foo/bar", "v1.2.3").unwrap();
        assert_eq!(b, "https://raw.githubusercontent.com/foo/bar/v1.2.3/");
    }

    #[test]
    fn candidate_refs_pinned() {
        assert_eq!(candidate_refs(Some("v1.2.3")), vec!["v1.2.3"]);
    }

    #[test]
    fn candidate_refs_fallback_is_main_then_master() {
        assert_eq!(candidate_refs(None), vec!["main", "master"]);
    }

    /// In-memory [`HttpFetcher`] keyed by URL → `Option<body>`.
    /// `Some(body)` → 200; `None` → 404. Missing keys are treated as 404 too.
    struct FakeHttp(HashMap<String, Option<String>>);

    impl HttpFetcher for FakeHttp {
        fn fetch_text(&self, url: &str) -> anyhow::Result<Option<String>> {
            Ok(self.0.get(url).cloned().unwrap_or(None))
        }
    }

    #[test]
    fn fetch_remote_detail_happy_path() {
        let entry = WorkloadEntry {
            name: "x".into(),
            url: "https://github.com/foo/bar".into(),
            language: "Rust".into(),
            description: None,
            default_ref: None,
            status: "stable".into(),
            tags: vec![],
        };
        let manifest = r#"
name = "x"
language = "rust"
strategies = ["proptest"]

[[tasks]]
mutations = ["m_0000000_1"]
[tasks.injection]
kind = "patch"
files = ["src/lib.rs"]
patch = "patches/fix.patch"

[[tasks.tasks]]
property = "SomeProp"
"#;
        let mut map = HashMap::new();
        map.insert(
            "https://raw.githubusercontent.com/foo/bar/main/etna.toml".to_string(),
            Some(manifest.to_string()),
        );
        map.insert(
            "https://raw.githubusercontent.com/foo/bar/main/patches/fix.patch".to_string(),
            Some("diff --git a/x b/x\n".to_string()),
        );
        map.insert(
            "https://raw.githubusercontent.com/foo/bar/main/README.md".to_string(),
            None,
        );
        let http = FakeHttp(map);

        match fetch_remote_detail(&entry, &http) {
            FetchOutcome::Ok { detail } => {
                assert_eq!(detail.manifest.name, "x");
                assert_eq!(detail.patches.len(), 1);
                assert!(detail.patches.contains_key("patches/fix.patch"));
                assert!(detail.readme_md.is_none());
                assert!(detail.bugs_md.is_some(), "docs should regenerate");
            }
            other => panic!("expected Ok, got {:?}", other),
        }
    }

    #[test]
    fn fetch_remote_detail_falls_back_to_master() {
        // A repo whose default branch is `master` — main 404s, master 200s.
        // Both manifest and patch should be pulled from the master ref.
        let entry = WorkloadEntry {
            name: "cratefork".into(),
            url: "https://github.com/foo/bar".into(),
            language: "Rust".into(),
            description: None,
            default_ref: None,
            status: "stable".into(),
            tags: vec![],
        };
        let manifest = r#"
name = "cratefork"
language = "rust"
strategies = ["proptest"]

[[tasks]]
mutations = ["m_0000000_1"]
[tasks.injection]
kind = "patch"
files = ["src/lib.rs"]
patch = "patches/fix.patch"

[[tasks.tasks]]
property = "P"
"#;
        let mut map = HashMap::new();
        // main/ etna.toml is absent — simulate by leaving it out of the map.
        map.insert(
            "https://raw.githubusercontent.com/foo/bar/master/etna.toml".to_string(),
            Some(manifest.to_string()),
        );
        map.insert(
            "https://raw.githubusercontent.com/foo/bar/master/patches/fix.patch".to_string(),
            Some("diff --git a/x b/x\n".to_string()),
        );
        let http = FakeHttp(map);

        match fetch_remote_detail(&entry, &http) {
            FetchOutcome::Ok { detail } => {
                assert_eq!(detail.manifest.name, "cratefork");
                assert_eq!(detail.patches.len(), 1);
            }
            other => panic!("expected Ok via master fallback, got {:?}", other),
        }
    }

    #[test]
    fn fetch_remote_detail_missing_manifest() {
        let entry = WorkloadEntry {
            name: "gone".into(),
            url: "https://github.com/foo/bar".into(),
            language: "Rust".into(),
            description: None,
            default_ref: None,
            status: "stable".into(),
            tags: vec![],
        };
        let http = FakeHttp(HashMap::new());
        match fetch_remote_detail(&entry, &http) {
            FetchOutcome::MissingManifest => (),
            other => panic!("expected MissingManifest, got {:?}", other),
        }
    }

    #[test]
    fn fetch_remote_detail_bad_url() {
        let entry = WorkloadEntry {
            name: "x".into(),
            url: "https://gitlab.com/foo/bar".into(),
            language: "Rust".into(),
            description: None,
            default_ref: None,
            status: "stable".into(),
            tags: vec![],
        };
        let http = FakeHttp(HashMap::new());
        match fetch_remote_detail(&entry, &http) {
            FetchOutcome::Err(_) => (),
            other => panic!("expected Err, got {:?}", other),
        }
    }
}
