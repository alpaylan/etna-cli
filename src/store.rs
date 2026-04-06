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
        self.metrics = content
            .lines()
            .filter_map(|line| {
                if line.trim().is_empty() {
                    None
                } else {
                    serde_json::from_str::<Metric>(line).ok()
                }
            })
            .collect();
        Ok(())
    }

    pub(crate) fn push(&mut self, metric: Metric) -> anyhow::Result<usize> {
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
                retained_metrics.push(metric.clone());
                if i > 0 {
                    writer.write_all(b"\n").expect("Failed to write newline");
                }
                serde_json::to_writer(&mut writer, metric)
                    .expect("Failed to write metric to store file");
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
