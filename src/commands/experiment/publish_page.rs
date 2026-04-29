//! `etna experiment publish-page` — bundle an experiment's results plus the
//! workload's manifest into a directory ready to drop onto Cloudflare Pages.
//!
//! Output layout:
//! ```text
//! <out>/
//!   index.html        — small landing page: workload description card,
//!                        summary stats, iframe(report.html)
//!   report.html       — output of `etna experiment report`
//!   report.json       — same metrics payload, exposed for /json
//!   store.jsonl       — verbatim per-trial rows
//!   etna.toml         — copy of the workload manifest
//! ```
//!
//! For multi-workload experiments (rare in CI), each workload after the
//! first goes under `<out>/workloads/<name>/`. Single-workload experiments
//! write at the root so `https://<workload>.pages.dev/` shows the page
//! directly.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::experiment::report::{render_html_from_payload, ReportPayload};
use crate::error_context::Context;
use crate::experiment::ExperimentMetadata;
use crate::manager::Manager;
use crate::workload::WorkloadManifest;

/// Aggregate per-property status counts for the summary card on `index.html`.
fn summarise(payload: &ReportPayload, workload_name: &str) -> Vec<PropertySummary> {
    let mut by_prop: HashMap<String, StatusCounter> = HashMap::new();
    for row in &payload.metrics {
        let row_workload = row
            .get("workload")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !row_workload.eq_ignore_ascii_case(workload_name) {
            continue;
        }
        let prop = row
            .get("property")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        let status = row
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        let entry = by_prop.entry(prop).or_default();
        match status.as_str() {
            "passed" => entry.passed += 1,
            "failed" => entry.failed += 1,
            _ => entry.other += 1,
        }
    }
    let mut out: Vec<PropertySummary> = by_prop
        .into_iter()
        .map(|(property, c)| PropertySummary {
            property,
            passed: c.passed,
            failed: c.failed,
            other: c.other,
        })
        .collect();
    out.sort_by(|a, b| a.property.cmp(&b.property));
    out
}

#[derive(Default)]
struct StatusCounter {
    passed: usize,
    failed: usize,
    other: usize,
}

struct PropertySummary {
    property: String,
    passed: usize,
    failed: usize,
    other: usize,
}

/// HTML escape for plaintext content. Bytes only — assumes UTF-8 input.
fn h(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_index(
    manifest: &WorkloadManifest,
    summary: &[PropertySummary],
    generated_at: &str,
) -> String {
    let total: usize = summary.iter().map(|p| p.passed + p.failed + p.other).sum();
    let total_failed: usize = summary.iter().map(|p| p.failed).sum();
    let total_passed: usize = summary.iter().map(|p| p.passed).sum();

    let summary_rows: String = summary
        .iter()
        .map(|p| {
            let bug_class = if p.failed > 0 { "found" } else { "missed" };
            format!(
                "<tr><td><code>{prop}</code></td><td class=\"num\">{pass}</td><td class=\"num\">{fail}</td><td class=\"num\">{other}</td><td class=\"verdict {bug_class}\">{verdict}</td></tr>",
                prop = h(&p.property),
                pass = p.passed,
                fail = p.failed,
                other = p.other,
                bug_class = bug_class,
                verdict = if p.failed > 0 { "bug found" } else { "—" },
            )
        })
        .collect();

    let crate_line = if let Some(c) = &manifest.crate_name {
        format!("<div class=\"meta\"><strong>Crate:</strong> <code>{}</code></div>", h(c))
    } else {
        String::new()
    };

    let base_commit_line = if let Some(b) = &manifest.base_commit {
        format!(
            "<div class=\"meta\"><strong>Base commit:</strong> <code>{}</code></div>",
            h(&b[..b.len().min(12)])
        )
    } else {
        String::new()
    };

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>{name} — etna results</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>
  :root {{ color-scheme: light dark; --fg:#1a1a1a; --bg:#fafafa; --muted:#666; --border:#ddd; --accent:#0353a4; --found:#b03a2e; --missed:#666; }}
  @media (prefers-color-scheme: dark) {{ :root {{ --fg:#e6e6e6; --bg:#0f0f0f; --muted:#999; --border:#333; --accent:#80b3ff; --found:#ff7766; --missed:#888; }} }}
  body {{ margin:0; font:14px/1.5 -apple-system,BlinkMacSystemFont,system-ui,sans-serif; color:var(--fg); background:var(--bg); }}
  header {{ padding: 24px max(24px, calc((100vw - 1100px) / 2)); border-bottom:1px solid var(--border); }}
  header h1 {{ margin: 0 0 8px; font-size: 22px; }}
  header .lang {{ display:inline-block; padding:2px 8px; background:var(--accent); color:#fff; border-radius:3px; font-size:12px; vertical-align:middle; }}
  main {{ padding: 24px max(24px, calc((100vw - 1100px) / 2)); }}
  .desc {{ white-space: pre-wrap; max-width: 70ch; color:var(--fg); margin: 0 0 24px; }}
  .meta {{ color:var(--muted); font-size:13px; margin: 4px 0; }}
  .summary {{ margin: 24px 0; border:1px solid var(--border); border-radius:6px; overflow:hidden; }}
  .summary h2 {{ margin:0; padding: 12px 16px; background: rgba(127,127,127,0.08); font-size:15px; }}
  .summary table {{ width:100%; border-collapse: collapse; }}
  .summary th, .summary td {{ padding: 8px 16px; text-align: left; border-top:1px solid var(--border); }}
  .summary th {{ font-weight:600; color:var(--muted); }}
  .summary .num {{ text-align:right; font-variant-numeric: tabular-nums; }}
  .summary .verdict.found {{ color:var(--found); font-weight:600; }}
  .summary .verdict.missed {{ color:var(--missed); }}
  .links a {{ display:inline-block; margin-right: 16px; color:var(--accent); }}
  .links {{ margin: 16px 0 24px; font-size: 13px; }}
  iframe {{ width:100%; height: 90vh; border:1px solid var(--border); border-radius:6px; background:#fff; }}
  footer {{ padding: 16px max(24px, calc((100vw - 1100px) / 2)); color:var(--muted); font-size:12px; border-top:1px solid var(--border); margin-top: 24px; }}
  code {{ font:13px/1.4 ui-monospace,"SF Mono",Menlo,monospace; }}
</style>
</head>
<body>
<header>
  <h1>{name} <span class="lang">{language}</span></h1>
  <div class="meta">Total: {total} trials · {total_passed} passed · {total_failed} failed</div>
</header>
<main>
  <p class="desc">{description}</p>
  {crate_line}
  {base_commit_line}
  <div class="summary">
    <h2>Per-property results</h2>
    <table>
      <thead><tr><th>Property</th><th class="num">passed</th><th class="num">failed</th><th class="num">other</th><th>verdict</th></tr></thead>
      <tbody>{rows}</tbody>
    </table>
  </div>
  <div class="links">
    <a href="./report.html">Interactive report ↗</a>
    <a href="./report.json">JSON (/report.json)</a>
    <a href="./store.jsonl">Raw store (/store.jsonl)</a>
    <a href="./etna.toml">etna.toml</a>
  </div>
  <iframe src="./report.html" title="Interactive report"></iframe>
</main>
<footer>Generated {generated_at} by <code>etna experiment publish-page</code>.</footer>
</body>
</html>
"#,
        name = h(&manifest.name),
        language = h(&manifest.language),
        description = h(manifest.description.as_deref().unwrap_or("")),
        crate_line = crate_line,
        base_commit_line = base_commit_line,
        total = total,
        total_passed = total_passed,
        total_failed = total_failed,
        rows = summary_rows,
        generated_at = h(generated_at),
    )
}

pub fn invoke(
    mut mgr: Manager,
    experiment: ExperimentMetadata,
    output: PathBuf,
    strip_counterexamples: bool,
) -> anyhow::Result<()> {
    fs::create_dir_all(&output)
        .with_context(|| format!("Failed to create output directory '{}'", output.display()))?;

    let payload = ReportPayload::build(&mut mgr, &experiment, strip_counterexamples)?;
    let html = render_html_from_payload(&payload)?;
    let json = payload.to_json_pretty()?;

    let workloads = experiment.workloads();
    if workloads.is_empty() {
        anyhow::bail!("Experiment '{}' has no workloads to publish", experiment.name);
    }

    // Single-workload experiments lay out at the dist root so
    // `<workload>.pages.dev/` is the workload page.
    if workloads.len() == 1 {
        write_workload_page(
            &output,
            &mgr,
            &experiment,
            &workloads[0].name,
            &payload,
            &html,
            &json,
        )?;
    } else {
        // First workload at root, rest under workloads/<name>/
        let mut iter = workloads.into_iter();
        let first = iter.next().expect("non-empty");
        write_workload_page(
            &output,
            &mgr,
            &experiment,
            &first.name,
            &payload,
            &html,
            &json,
        )?;
        for wl in iter {
            let sub = output.join("workloads").join(&wl.name);
            fs::create_dir_all(&sub).with_context(|| {
                format!("Failed to create directory '{}'", sub.display())
            })?;
            write_workload_page(&sub, &mgr, &experiment, &wl.name, &payload, &html, &json)?;
        }
    }

    tracing::info!("Published page to {}", output.display());
    Ok(())
}

fn write_workload_page(
    out: &Path,
    _mgr: &Manager,
    experiment: &ExperimentMetadata,
    workload_name: &str,
    payload: &ReportPayload,
    html: &str,
    json: &str,
) -> anyhow::Result<()> {
    fs::write(out.join("report.html"), html).context("Failed to write report.html")?;
    fs::write(out.join("report.json"), json).context("Failed to write report.json")?;

    // Copy the experiment's store (verbatim per-trial rows). Store may
    // have been written by a previous run; if it's missing, leave a stub.
    if experiment.store.is_file() {
        fs::copy(&experiment.store, out.join("store.jsonl"))
            .context("Failed to copy store.jsonl")?;
    } else {
        fs::write(out.join("store.jsonl"), "")
            .context("Failed to write empty store.jsonl")?;
    }

    let dir = experiment
        .workload_path(workload_name)
        .with_context(|| format!("Workload '{}' not found in experiment", workload_name))?;
    let manifest = WorkloadManifest::read(&dir).with_context(|| {
        format!("Failed to read etna.toml for workload '{}'", workload_name)
    })?;

    // Copy the raw etna.toml (pretty Rust-side serialisation would lose
    // user comments — the verbatim file is what consumers want).
    let etna_toml_src = dir.join("etna.toml");
    if etna_toml_src.is_file() {
        fs::copy(&etna_toml_src, out.join("etna.toml"))
            .context("Failed to copy etna.toml")?;
    }

    let summary = summarise(payload, &manifest.name);
    let index = render_index(&manifest, &summary, &payload.generated_at);
    fs::write(out.join("index.html"), index).context("Failed to write index.html")?;

    Ok(())
}
