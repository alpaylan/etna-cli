use std::{
    fs,
    io::{Seek, Write as _},
    path::PathBuf,
};

use anyhow::Ok;
use serde_derive::{Deserialize, Serialize};

use crate::error_context::Context;

#[derive(Debug, Deserialize, Serialize)]
pub struct Store {
    pub path: PathBuf,
    pub(crate) metrics: Vec<Metric>,
}

impl Store {
    pub fn new(path: PathBuf) -> anyhow::Result<Self> {
        // If the store file does not exist, create it
        if !path.exists() {
            tracing::trace!("store file does not exist, creating it");
            if let Some(parent) = path.parent() {
                tracing::trace!("creating parent directories for the store");
                std::fs::create_dir_all(parent)
                    .context("Failed to create parent directories for the store")?;
            }
            tracing::trace!("creating store file at {}", path.display());
            std::fs::File::create(&path).context("Failed to create store file")?;
        }

        Ok(Store {
            metrics: Vec::new(),
            path,
        })
    }

    pub(crate) fn load_metrics(&mut self) -> anyhow::Result<()> {
        let content = std::fs::read_to_string(&self.path).context("Failed to read store file")?;
        let mut metrics = Vec::new();
        let mut dropped = 0usize;
        for (i, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Metric>(line) {
                Result::Ok(metric) => metrics.push(metric),
                Err(e) => {
                    dropped += 1;
                    tracing::warn!(
                        "Skipping unparseable metric at {}:{}: {}",
                        self.path.display(),
                        i + 1,
                        e
                    );
                }
            }
        }
        if dropped > 0 {
            tracing::warn!(
                "Dropped {} unparseable line(s) while loading '{}'; affected trials will be treated as incomplete",
                dropped,
                self.path.display()
            );
        }
        self.metrics = metrics;
        Ok(())
    }

    pub(crate) fn push(&mut self, mut metric: Metric) -> anyhow::Result<usize> {
        metric.data = canonicalize_fields(metric.data);

        let store_file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        let mut writer = std::io::BufWriter::new(store_file);
        serde_json::to_writer(&mut writer, &metric)?;
        writer.write_all(b"\n")?;
        writer.flush()?;

        self.metrics.push(metric);

        Ok(self.metrics.len())
    }

    pub(crate) fn retain<F>(&mut self, f: F)
    where
        F: Fn(&Metric) -> bool,
    {
        let mut store_file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)
            .expect("Failed to open store file for writing");
        let mut writer = std::io::BufWriter::new(&mut store_file);
        writer
            .seek(std::io::SeekFrom::Start(0))
            .expect("Failed to seek to start of store file");
        let mut retained_metrics = Vec::new();

        for (i, metric) in self.metrics.iter().enumerate() {
            if f(metric) {
                let canonical = Metric {
                    data: canonicalize_fields(metric.data.clone()),
                    hash: metric.hash.clone(),
                };
                if i > 0 {
                    writer.write_all(b"\n").expect("Failed to write newline");
                }
                serde_json::to_writer(&mut writer, &canonical)
                    .expect("Failed to write metric to store file");
                retained_metrics.push(canonical);
            }
        }
        writer.flush().expect("Failed to flush writer");
        self.metrics = retained_metrics;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Metric {
    pub data: serde_json::Map<String, serde_json::Value>,
    pub hash: String,
}

/// Canonical order for known fields. Any keys not in this list are placed
/// alphabetically between the known prefix and `counterexample`, which is
/// always written last when present.
const CANONICAL_ORDER: &[&str] = &[
    "experiment",
    "workload",
    "language",
    "producer_language",
    "producer_workload",
    "strategy",
    "property",
    "mutations",
    "mode",
    "trial",
    "timeout",
    "timestamp",
    "status",
    "passed",
    "tests",
    "discarded",
    "discards",
    "shrinks",
    "samples",
    "time",
    "execution_time",
    "generation_time",
    "shrinking_time",
    "cross",
    "error",
];

const COUNTEREXAMPLE: &str = "counterexample";

fn canonicalize_fields(
    mut data: serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut ordered = serde_json::Map::with_capacity(data.len());

    for key in CANONICAL_ORDER {
        if let Some(v) = data.remove(*key) {
            ordered.insert((*key).to_string(), v);
        }
    }

    let counterexample = data.remove(COUNTEREXAMPLE);

    let mut leftover: Vec<(String, serde_json::Value)> = data.into_iter().collect();
    leftover.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, v) in leftover {
        ordered.insert(k, v);
    }

    if let Some(v) = counterexample {
        ordered.insert(COUNTEREXAMPLE.to_string(), v);
    }

    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_order_places_known_first_then_unknown_then_counterexample() {
        let input: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{
                "counterexample": "cx",
                "zzz_extra": 1,
                "tests": 10,
                "experiment": "exp",
                "aaa_extra": 2,
                "status": "ok",
                "language": "rust",
                "workload": "bst"
            }"#,
        )
        .unwrap();

        let ordered = canonicalize_fields(input);
        let keys: Vec<&str> = ordered.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            vec![
                "experiment",
                "workload",
                "language",
                "status",
                "tests",
                "aaa_extra",
                "zzz_extra",
                "counterexample",
            ]
        );
    }

    #[test]
    fn canonical_order_handles_missing_fields() {
        let input: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(r#"{"workload":"bst","experiment":"exp"}"#).unwrap();
        let keys: Vec<String> = canonicalize_fields(input).keys().cloned().collect();
        assert_eq!(keys, vec!["experiment".to_string(), "workload".to_string()]);
    }
}
