use std::io::Write;
use std::path::PathBuf;

use base64::Engine;

use crate::{
    error_context::Context, experiment::ExperimentMetadata, manager::Manager,
    workload::WorkloadManifest,
};

const TEMPLATE: &str = include_str!("../../../templates/report.html");

/// Build a map `workload_name_lower -> manifest.tasks` by reading each
/// registered workload's `etna.toml`. Shape matches what the report template
/// expects: a list of `{mutations, tasks: [{property, …}]}`.
fn load_workload_docs(
    experiment: &ExperimentMetadata,
) -> serde_json::Map<String, serde_json::Value> {
    let mut result = serde_json::Map::new();
    for wl in experiment.workloads() {
        let Some(dir) = experiment.workload_path(&wl.name) else {
            continue;
        };
        let manifest = match WorkloadManifest::read(&dir) {
            Ok(m) => m,
            Err(e) => {
                tracing::debug!("Skipping '{}': {}", dir.display(), e);
                continue;
            }
        };
        match serde_json::to_value(&manifest.tasks) {
            Ok(val) => {
                result.insert(manifest.name.to_lowercase(), val);
            }
            Err(e) => {
                tracing::debug!(
                    "Failed to serialize manifest tasks for '{}': {}",
                    manifest.name,
                    e
                );
            }
        }
    }
    result
}

/// Publish an HTML file as a public GitHub Gist and return the viewable URL.
fn publish_gist(path: &std::path::Path) -> anyhow::Result<String> {
    let output = std::process::Command::new("gh")
        .args(["gist", "create", "--public"])
        .arg(path)
        .output()
        .context(
            "Failed to run `gh gist create`. Is the GitHub CLI installed and authenticated?",
        )?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("gh gist create failed: {}", stderr.trim());
    }

    let gist_url = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Extract gist ID from URL like https://gist.github.com/user/abc123
    let gist_id = gist_url
        .rsplit('/')
        .next()
        .context("Could not parse gist ID from URL")?;

    let filename = path
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("report.html");

    let view_url = format!(
        "https://etna-reports.alpkeles99.workers.dev/{}/{}",
        gist_id, filename
    );

    Ok(format!("Gist: {}\nView: {}", gist_url, view_url))
}

/// The serialisable payload that backs both `render_html` and any JSON-only
/// sink (e.g. the `--json-output` flag). Separated so the two outputs share
/// the exact same underlying data.
pub struct ReportPayload {
    pub experiment_name: String,
    pub generated_at: String,
    pub metrics: Vec<serde_json::Value>,
    pub workload_docs: serde_json::Map<String, serde_json::Value>,
}

impl ReportPayload {
    /// Read the experiment store and assemble the payload. Side effect:
    /// loads metrics into `mgr`'s store.
    pub fn build(
        mgr: &mut Manager,
        experiment: &ExperimentMetadata,
        strip_counterexamples: bool,
    ) -> anyhow::Result<Self> {
        mgr.set_store_path(experiment.store.clone())?;
        mgr.require_store_mut()?.load_metrics()?;

        let metrics: Vec<serde_json::Value> = mgr
            .require_store()?
            .metrics
            .iter()
            .map(|m| {
                let mut obj = m.data.clone();
                if strip_counterexamples {
                    obj.remove("counterexample");
                }
                obj.insert(
                    "hash".to_string(),
                    serde_json::Value::String(m.hash.clone()),
                );
                serde_json::Value::Object(obj)
            })
            .collect();

        let workload_docs = load_workload_docs(experiment);
        let generated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

        Ok(Self {
            experiment_name: experiment.name.clone(),
            generated_at,
            metrics,
            workload_docs,
        })
    }

    /// Serialise as a single JSON document. Stable shape used by the
    /// `--json-output` flag and by `experiment publish-page` for `/json`.
    pub fn to_json_pretty(&self) -> anyhow::Result<String> {
        let body = serde_json::json!({
            "experiment": self.experiment_name,
            "generated_at": self.generated_at,
            "workload_docs": self.workload_docs,
            "metrics": self.metrics,
        });
        Ok(serde_json::to_string_pretty(&body)?)
    }
}

/// Render the report HTML for an experiment without writing it to disk.
/// Side effects: loads the experiment store into `mgr`.
///
/// When `strip_counterexamples` is true, per-trial `counterexample` fields
/// are dropped from the embedded payload — useful for keeping the HTML
/// under the GitHub Gist size ceiling at the cost of losing them in the
/// interactive report.
pub fn render_html(
    mgr: &mut Manager,
    experiment: &ExperimentMetadata,
    strip_counterexamples: bool,
) -> anyhow::Result<String> {
    let payload = ReportPayload::build(mgr, experiment, strip_counterexamples)?;
    render_html_from_payload(&payload)
}

/// Same as `render_html` but takes a pre-built payload, so callers (e.g.
/// `publish-page`) can render HTML and JSON from the same source without
/// re-reading the store.
pub fn render_html_from_payload(payload: &ReportPayload) -> anyhow::Result<String> {
    let metrics_json_raw = serde_json::to_string(&payload.metrics)?;

    // Gzip-compress and base64-encode metrics to keep the HTML under GitHub's 1MB API limit
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(metrics_json_raw.as_bytes())?;
    let compressed = encoder.finish()?;
    let metrics_b64 = base64::engine::general_purpose::STANDARD.encode(&compressed);

    tracing::debug!(
        "Metrics: {} raw -> {} compressed -> {} base64",
        metrics_json_raw.len(),
        compressed.len(),
        metrics_b64.len()
    );
    let experiment_name_json = serde_json::to_string(&payload.experiment_name)?;
    let workload_docs_json = serde_json::to_string(&payload.workload_docs)?;
    let generated_at_json = serde_json::to_string(&payload.generated_at)?;

    // Render template with minijinja
    let mut env = minijinja::Environment::new();
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
    env.add_template("report.html", TEMPLATE)?;

    let tmpl = env.get_template("report.html")?;
    let html = tmpl.render(minijinja::context! {
        experiment_name => &payload.experiment_name,
        metrics_b64 => &metrics_b64,
        experiment_name_json => &experiment_name_json,
        workload_docs_json => &workload_docs_json,
        generated_at_json => &generated_at_json,
    })?;

    Ok(html)
}

pub fn invoke(
    mut mgr: Manager,
    experiment: ExperimentMetadata,
    output: Option<PathBuf>,
    publish: bool,
    strip_counterexamples: bool,
    json_output: Option<PathBuf>,
) -> anyhow::Result<()> {
    let payload = ReportPayload::build(&mut mgr, &experiment, strip_counterexamples)?;
    let html = render_html_from_payload(&payload)?;

    let output_path = output.unwrap_or_else(|| experiment.path.join("report.html"));
    std::fs::write(&output_path, &html).context("Failed to write report HTML")?;
    tracing::info!("Report written to {}", output_path.display());

    if let Some(json_path) = json_output {
        let body = payload.to_json_pretty()?;
        std::fs::write(&json_path, body).context("Failed to write report JSON")?;
        tracing::info!("Report JSON written to {}", json_path.display());
    }

    if publish {
        let result = publish_gist(&output_path)?;
        tracing::info!("{}", result);
    }

    Ok(())
}
