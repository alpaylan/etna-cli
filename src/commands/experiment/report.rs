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

/// Render the report HTML for an experiment without writing it to disk.
/// Side effects: loads the experiment store into `mgr`.
pub fn render_html(mgr: &mut Manager, experiment: &ExperimentMetadata) -> anyhow::Result<String> {
    // Load metrics
    mgr.set_store_path(experiment.store.clone())?;
    mgr.require_store_mut()?.load_metrics()?;

    // Counterexamples can run to thousands of characters per trial; keeping
    // them blows up the embedded JSON and the webview's parse time, so drop
    // them from the report payload.
    let metrics: Vec<serde_json::Value> = mgr
        .require_store()?
        .metrics
        .iter()
        .map(|m| {
            let mut obj = m.data.clone();
            obj.remove("counterexample");
            obj.insert(
                "hash".to_string(),
                serde_json::Value::String(m.hash.clone()),
            );
            serde_json::Value::Object(obj)
        })
        .collect();

    // Load workload docs for mutation matrix filtering
    let workload_docs = load_workload_docs(experiment);

    let metrics_json_raw = serde_json::to_string(&metrics)?;

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
    let experiment_name_json = serde_json::to_string(&experiment.name)?;
    let workload_docs_json = serde_json::to_string(&workload_docs)?;
    let generated_at = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let generated_at_json = serde_json::to_string(&generated_at)?;

    // Render template with minijinja
    let mut env = minijinja::Environment::new();
    env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);
    env.add_template("report.html", TEMPLATE)?;

    let tmpl = env.get_template("report.html")?;
    let html = tmpl.render(minijinja::context! {
        experiment_name => &experiment.name,
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
) -> anyhow::Result<()> {
    let html = render_html(&mut mgr, &experiment)?;

    let output_path = output.unwrap_or_else(|| experiment.path.join("report.html"));
    std::fs::write(&output_path, &html).context("Failed to write report HTML")?;

    tracing::info!("Report written to {}", output_path.display());

    if publish {
        let result = publish_gist(&output_path)?;
        tracing::info!("{}", result);
    }

    Ok(())
}
