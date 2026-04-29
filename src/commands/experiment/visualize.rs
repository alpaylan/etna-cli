use std::{fmt::Display, io::Write, path::Path};

use ab_glyph::{Font, FontRef, ScaleFont as _};
use image::{Rgb, RgbImage};
use imageproc::{drawing::draw_filled_rect_mut, rect::Rect};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{
    error_context::Context,
    experiment::{ExperimentMetadata, Test},
    manager::Manager,
    open_pbt_format::Status,
    store::Store,
};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum MetricType {
    Discards,
    Tests,
    Shrinks,
    Time,
}

impl Display for MetricType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            MetricType::Discards => write!(f, "discards"),
            MetricType::Tests => write!(f, "tests"),
            MetricType::Shrinks => write!(f, "shrinks"),
            MetricType::Time => write!(f, "time"),
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum AggregationType {
    Sum,
    Avg,
}

impl Display for AggregationType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AggregationType::Sum => write!(f, "sum"),
            AggregationType::Avg => write!(f, "avg"),
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum VisualizationType {
    Bucket,
    Bar,
    Line,
}

pub(crate) fn write_row<W: std::io::Write>(
    writer: &mut W,
    metric: &serde_json::Value,
    aggby: &[String],
) -> anyhow::Result<()> {
    let mut row = vec![];
    for a in aggby {
        row.push(metric.get(a).map_or("".to_string(), |v| v.to_string()));
    }
    row.push(metric.get("tests").map_or("NaN".to_string(), |v| {
        v.as_f64()
            .map_or("NaN".to_string(), |t| format!("{:.2}", t))
    }));
    row.push(metric.get("discards").map_or("NaN".to_string(), |v| {
        v.as_f64()
            .map_or("NaN".to_string(), |t| format!("{:.2}", t))
    }));
    row.push(metric.get("shrinks").map_or("NaN".to_string(), |v| {
        v.as_f64()
            .map_or("NaN".to_string(), |t| format!("{:.2}", t))
    }));
    row.push(metric.get("time").map_or("NaN".to_string(), |v| {
        v.as_f64()
            .map_or("NaN".to_string(), |t| format!("{:.4}", t))
    }));

    tracing::debug!("Writing row: {:?}", row);
    writer
        .write_all(row.join(",").as_bytes())
        .context("Failed to write row")?;

    writer.write_all(b"\n").context("Failed to write newline")
}

#[allow(clippy::too_many_arguments)]
pub fn invoke(
    _mgr: Manager,
    experiment: ExperimentMetadata,
    figure_name: String,
    tests: Vec<String>,
    groupby: Vec<String>,
    aggby: Vec<String>,
    metric: MetricType,
    buckets: Vec<f64>,
    max: Option<f64>,
    typ_: VisualizationType,
    hatched: Vec<usize>,
) -> anyhow::Result<()> {
    tracing::trace!("visualizing experiment with name '{:?}'", experiment.name);

    if tests.is_empty() {
        anyhow::bail!("No tests provided. Please specify at least one test to run.");
    }

    let mut store = Store::new(experiment.store.clone())
        .context("Failed to load the store for visualization")?;
    store.load_metrics()?;

    let agg_metrics = get_agg_metrics(&experiment, &store, &figure_name, &tests, &aggby)?;

    tracing::trace!("Aggregated metrics: {:#?}", agg_metrics);

    tracing::debug!("Number of aggregated metrics: {}", agg_metrics.len());

    tracing::debug!("Groupby fields: {:#?}", groupby);

    match typ_ {
        VisualizationType::Bucket => draw_bucket_chart(
            &experiment,
            &figure_name,
            agg_metrics,
            groupby,
            metric,
            buckets,
            hatched,
        ),
        VisualizationType::Bar => {
            draw_bar_chart(
                &experiment,
                figure_name,
                agg_metrics,
                groupby,
                AggregationType::Sum, // Assuming sum for bar chart
                AggregationType::Avg, // Assuming avg for bar chart
                metric,
                max,
            )
        }
        VisualizationType::Line => {
            // Placeholder for line chart implementation
            anyhow::bail!("Line chart visualization is not implemented yet.");
        }
    }
}

fn get_agg_metrics(
    experiment: &ExperimentMetadata,
    store: &Store,
    figure_name: &str,
    tests: &[String],
    aggby: &[String],
) -> anyhow::Result<Vec<serde_json::Map<std::string::String, serde_json::Value>>> {
    let figures_path = experiment.path.join("figures");

    let raw_data_path = figures_path.join(format!("{}_raw.csv", figure_name));
    let mut raw_data_file = std::fs::File::create(&raw_data_path).context(format!(
        "Failed to create raw data file at {}",
        raw_data_path.display()
    ))?;

    let mut top_row = aggby
        .iter()
        .map(|a| a.as_str())
        .chain(["discards", "tests", "shrinks", "time"].iter().copied())
        .collect::<Vec<_>>()
        .join(",");
    top_row.push('\n');
    raw_data_file.write_all(top_row.as_bytes())?;

    let tests = tests
        .iter()
        .map(|test| {
            let test_path = experiment
                .path
                .join("tests")
                .join(test)
                .with_extension("json");
            let test: Vec<Test> =
                serde_json::from_str(&std::fs::read_to_string(&test_path).with_context(|| {
                    format!(
                        "Could not read test at {}, available tests are:\n{}",
                        test_path.display(),
                        std::fs::read_dir(experiment.path.join("tests"))
                            .unwrap()
                            .filter_map(|e| e.ok())
                            .map(|e| e.file_name().to_string_lossy().to_string())
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                })?)?;
            Result::<Vec<Test>, anyhow::Error>::Ok(test)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let tests: Vec<Test> = tests.into_iter().flatten().collect();

    if tests.is_empty() {
        anyhow::bail!("No tests loaded for visualization");
    }

    tracing::trace!("Loaded {} tests for visualization", tests.len());

    let metrics = tests
        .iter()
        .flat_map(|test| {
            store.metrics.iter().filter_map(|m| {
                let workload = m.data.get("workload").and_then(serde_json::Value::as_str)?;
                let mutations = m
                    .data
                    .get("mutations")
                    .and_then(serde_json::Value::as_array)?;
                let strategy = m.data.get("strategy").and_then(serde_json::Value::as_str)?;
                let property = m.data.get("property").and_then(serde_json::Value::as_str)?;
                let mode = m.data.get("mode").and_then(serde_json::Value::as_str)?;

                // For Cross mode, the metric's workload = consumer's.
                let test_wl = match &test.mode {
                    crate::experiment::Mode::Cross { consumer, .. } => consumer.workload.as_str(),
                    _ => test.workload.as_str(),
                };

                let result = test_wl == workload
                    && &test.mutations == mutations
                    && test.tasks.iter().any(|task| {
                        task.get("strategy").and_then(serde_json::Value::as_str) == Some(strategy)
                            && task.get("property").and_then(serde_json::Value::as_str)
                                == Some(property)
                    })
                    && test.mode.name() == mode;

                if result {
                    Some(m)
                } else {
                    None
                }
            })
        })
        .collect::<Vec<_>>();

    tracing::debug!("Aggregated metrics by: {:#?}", aggby);

    tracing::trace!("metrics: {:#?}", &metrics[..]);

    let aggs = aggby
        .iter()
        .map(|g| {
            metrics
                .iter()
                .map(|m| {
                    m.data
                        .get(g)
                        .unwrap_or_else(|| panic!("Aggby field '{g}' not found"))
                })
                .unique()
                .collect::<Vec<_>>()
        })
        .multi_cartesian_product()
        .collect::<Vec<Vec<_>>>();

    tracing::trace!("Aggregations: {:#?}", aggs);

    let agg_metrics = aggs
        .iter()
        .filter_map(|agg| {
            let agg_metrics = metrics
                .iter()
                .filter(|m| {
                    aggby.iter().enumerate().all(|(i, g)| {
                        tracing::trace!(
                            "Checking if metric {:?} has groupby field {}: {:?} == {:?}",
                            m.data,
                            g,
                            agg[i],
                            m.data.get(g)
                        );
                        m.data.get(g).is_some_and(|v| agg[i] == v)
                    })
                })
                .collect::<Vec<_>>();

            if agg_metrics.is_empty() {
                tracing::trace!("No metrics found for group: {:?}", agg);
                return None;
            }
            tracing::trace!("Group: {:#?}", agg);
            tracing::trace!("Number of metrics in agg: {}", agg_metrics.len());
            // if it timed out or aborted, we want to return NaN.

            let timed_out = agg_metrics.iter().find_map(|m| {
                m.data
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|t| {
                        if t == Status::TimedOut.to_string().as_str() {
                            tracing::debug!("Metric {:?} has timed out", m.data);
                            m.data.get("timeout").and_then(serde_json::Value::as_f64)
                        } else {
                            None
                        }
                    })
            });

            if let Some(timeout) = timed_out {
                tracing::warn!("Some metrics in group {:?} timed out", agg);
                let mut data = serde_json::Map::new();
                for (i, key) in aggby.iter().enumerate() {
                    if let Some(v) = agg.get(i) {
                        data.insert(key.clone(), (*v).clone());
                    }
                }
                data.insert("discards".to_string(), serde_json::Value::from(f64::NAN));
                data.insert("tests".to_string(), serde_json::Value::from(f64::NAN));
                data.insert("shrinks".to_string(), serde_json::Value::from(f64::NAN));
                data.insert(
                    "time".to_string(),
                    serde_json::Value::from(format!("{timeout}s")),
                );
                let data = serde_json::Value::Object(data);
                tracing::trace!("Returning timeout data: {:#?}", data);
                let _ = write_row(&mut raw_data_file, &data, aggby);
                return data.as_object().cloned();
            }

            let aborted = agg_metrics.iter().find_map(|m| {
                m.data
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|t| {
                        if t == Status::Aborted.to_string().as_str() {
                            tracing::debug!("Metric {:?} has aborted", m.data);
                            Some(true)
                        } else {
                            None
                        }
                    })
            });

            if let Some(true) = aborted {
                tracing::error!("Some metrics in group {:?} were aborted", agg);
                let mut data = serde_json::Map::new();
                for (i, key) in aggby.iter().enumerate() {
                    if let Some(v) = agg.get(i) {
                        data.insert(key.clone(), (*v).clone());
                    }
                }
                data.insert("discards".to_string(), serde_json::Value::from(f64::NAN));
                data.insert("tests".to_string(), serde_json::Value::from(f64::NAN));
                data.insert("shrinks".to_string(), serde_json::Value::from(f64::NAN));
                data.insert("time".to_string(), serde_json::Value::from(f64::NAN));
                let data = serde_json::Value::Object(data);
                tracing::trace!("Returning aborted data: {:#?}", data);
                let _ = write_row(&mut raw_data_file, &data, aggby);
                return data.as_object().cloned();
            }

            // let aborted = agg_metrics.iter().any(|m| {
            //     m.data
            //         .get("status")
            //         .map(serde_json::Value::as_str)
            //         .unwrap()
            //         .map(|t| t == "aborted")
            //         .unwrap()
            // });
            // if aborted {
            //     tracing::warn!("Some metrics in group {:?} were aborted", agg);
            //     let data = serde_json::json!({
            //         "language": agg[0],
            //         "workload": agg[1],
            //         "strategy": agg[2],
            //         "property": agg[3],
            //         "mutations": agg[4],
            //         "cross": agg[5],
            //         "discards": f64::NAN,
            //         "tests": f64::NAN,
            //         "shrinks": f64::NAN,
            //         "time": f64::NAN,
            //     });
            //     tracing::trace!("Returning aborted data: {:#?}", data);
            //     let _ = write_row(&mut raw_data_file, &data, &aggby);
            //     return data.as_object().cloned();
            // }

            // let finished_or_gave_up = agg_metrics.iter().any(|m| {
            //     m.data
            //         .get("status")
            //         .map(serde_json::Value::as_str)
            //         .unwrap()
            //         .map(|t| t == "finished" || t == "gave_up")
            //         .unwrap()
            // });

            // if finished_or_gave_up {
            //     tracing::warn!("Some metrics in group {:?} finished or gave up", agg);
            //     let data = serde_json::json!({
            //         "language": agg[0],
            //         "workload": agg[1],
            //         "strategy": agg[2],
            //         "property": agg[3],
            //         "mutations": agg[4],
            //         "cross": agg[5],
            //         "discards": f64::NAN,
            //         "tests": f64::NAN,
            //         "shrinks": f64::NAN,
            //         "time": f64::NAN,
            //     });
            //     tracing::trace!("Returning finished or gave up data: {:#?}", data);
            //     let _ = write_row(&mut raw_data_file, &data, &aggby);
            //     return data.as_object().cloned();
            // }

            let sums: (f64, f64, f64, f64) =
                agg_metrics.iter().fold((0.0, 0.0, 0.0, 0.0), |mut acc, m| {
                    acc.0 = m
                        .data
                        .get("discards")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0);
                    acc.1 += m
                        .data
                        .get("tests")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0);
                    acc.2 += m
                        .data
                        .get("shrinks")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(0.0);
                    acc.3 += m
                        .data
                        .get("time")
                        .and_then(serde_json::Value::as_str)
                        .into_iter()
                        .flat_map(parse_duration::parse)
                        .next()
                        .map(|d| d.as_secs_f64())
                        .unwrap_or_else(|| panic!("Failed to parse time for metric: {:?}", m));
                    acc
                });
            let avgs = (
                sums.0 / agg_metrics.len() as f64,
                sums.1 / agg_metrics.len() as f64,
                sums.2 / agg_metrics.len() as f64,
                sums.3 / agg_metrics.len() as f64,
            );

            tracing::debug!(
                "Aggregated metrics for group {:?}: \n\
            Discards: {:.2}, Tests: {:.2}, Shrinks: {:.2}, Time: {:.4} seconds",
                agg,
                avgs.0,
                avgs.1,
                avgs.2,
                avgs.3
            );

            // Build the row dynamically from aggby — its ordering can vary
            // (default is workload/strategy/property/mutations/mode; not every
            // call specifies "cross" or "language") so a hard-coded index map
            // panics for any aggby shorter than 6 entries.
            let mut data = serde_json::Map::new();
            for (i, key) in aggby.iter().enumerate() {
                if let Some(v) = agg.get(i) {
                    data.insert(key.clone(), (*v).clone());
                }
            }
            data.insert(
                "discards".to_string(),
                serde_json::Value::from(avgs.0),
            );
            data.insert("tests".to_string(), serde_json::Value::from(avgs.1));
            data.insert("shrinks".to_string(), serde_json::Value::from(avgs.2));
            data.insert("time".to_string(), serde_json::Value::from(avgs.3));
            let data = serde_json::Value::Object(data);
            tracing::debug!("Writing to {}: {:#?}", raw_data_path.display(), data);
            let _ = write_row(&mut raw_data_file, &data, aggby);

            data.as_object().cloned()
        })
        .collect::<Vec<_>>();

    tracing::trace!("Aggregated metrics: {:#?}", agg_metrics);

    Ok(agg_metrics)
}

fn draw_bucket_chart(
    experiment: &ExperimentMetadata,
    figure_name: &str,
    agg_metrics: Vec<Map<String, Value>>,
    groupby: Vec<String>,
    metric: MetricType,
    mut buckets: Vec<f64>,
    hatched_indices: Vec<usize>,
) -> anyhow::Result<()> {
    let mut groups = groupby
        .iter()
        .map(|g| {
            agg_metrics
                .iter()
                .map(|m| {
                    m.get(g)
                        .unwrap_or_else(|| panic!("Groupby field '{g}' not found"))
                })
                .unique()
                .collect::<Vec<_>>()
        })
        .multi_cartesian_product()
        .collect::<Vec<Vec<_>>>();

    // Remove empty groups, meaning groups that filter no metrics.
    groups.retain(|g| {
        agg_metrics.iter().any(|m| {
            groupby
                .iter()
                .enumerate()
                .all(|(i, g_)| m.get(g_).is_some_and(|v| g[i] == v))
        })
    });

    // The following code creates the sizes of the image;
    // Image width is fixed at 4000px.
    let width = 4000.0;
    // The ratio of the height is calculated based on the number of groups.
    let ratio = 0.2 + 0.1 * (groups.len() as f64);
    let height = (width * ratio).round();
    // The height of each bucket is calculated based on the number of groups.
    // The margins between the buckets are 10% of the bucket height.
    // For N groups, the height of the image is;
    // height  = N * bucket_height + (N+1) / 10 * bucket_height.
    // --> bucket_height = height / (N * 1.1 + 0.1).
    let bucket_height = height / (groups.len() as f64 * 1.1 + 0.1);
    // The margin between the buckets is 10% of the bucket height.
    let margin = bucket_height * 0.1;

    let mut image = RgbImage::new(width as u32, height as u32);

    draw_filled_rect_mut(
        &mut image,
        Rect::at(0, 0).of_size(width as u32, height as u32),
        Rgb([255, 255, 255]),
    );

    let colors = [
        Rgb([0x00, 0x00, 0x00]), // black
        Rgb([0x90, 0x0D, 0x0D]), // red
        Rgb([0xDC, 0x5F, 0x00]), // orange
        Rgb([0x24, 0x37, 0x63]), // blue
        Rgb([0x43, 0x6E, 0x4F]), // green
        Rgb([0x47, 0x09, 0x38]), // purple
        Rgb([0xD6, 0x1C, 0x4E]), // pink
        Rgb([0x33, 0x47, 0x56]), // dark blue
        Rgb([0x29, 0x00, 0x01]), // dark brown
        Rgb([0x00, 0x00, 0x00]), // black
    ];

    for (i, group) in groups.iter().enumerate() {
        tracing::trace!("Processing group {i}: {:?}", group);
        let group_metrics = agg_metrics
            .iter()
            .filter(|m| {
                groupby
                    .iter()
                    .enumerate()
                    .all(|(i, g)| m.get(g).is_some_and(|v| group[i] == v))
            })
            .collect::<Vec<_>>();

        if buckets[0] != 0.0 {
            buckets.insert(0, 0.0);
        }
        if buckets.last() != Some(&f64::INFINITY) {
            buckets.push(f64::INFINITY);
        }

        // create buckets between b[0-1], b[1-2], ..., b[n-1-n]
        #[allow(clippy::type_complexity)]
        let mut buckets: Vec<((f64, f64), Vec<Map<String, Value>>)> = buckets
            .windows(2)
            .map(|w| ((w[0], w[1]), vec![]))
            .collect::<Vec<_>>();

        for metric in &group_metrics {
            let time = metric
                .get("time")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(f64::MAX);

            for ((start, end), metrics) in &mut buckets {
                if time >= *start && time < *end {
                    metrics.push((*metric).clone());
                }
            }
        }

        tracing::trace!(
            "\n\tGroup: {:?}\n\tBuckets: {:?}",
            group,
            buckets
                .iter()
                .map(|((start, end), values)| format!(
                    "{:.2} - {:.2}: {:?}",
                    start,
                    end,
                    values
                        .iter()
                        .map(|m| m
                            .get(metric.to_string().as_str())
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or(0.0))
                        .collect::<Vec<_>>()
                ))
                .collect::<Vec<_>>()
        );

        for ((start, end), values) in &buckets {
            for m in values {
                tracing::debug!("Bucket ({:.2}, {:.2}) contains metric: {:?}", start, end, m);
            }
        }

        let buckets = buckets
            .into_iter()
            .map(|((start, end), values)| {
                (
                    (start, end),
                    values
                        .iter()
                        .map(|m| {
                            m.get(metric.to_string().as_str())
                                .and_then(serde_json::Value::as_f64)
                                .unwrap_or(0.0)
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();

        let cfg = BucketDrawConfig {
            width,
            height,
            margin,
            bucket_height,
            bucket_index: i,
            fill_color: colors[i % colors.len()],
            legend: false,
            hatched: hatched_indices.contains(&i),
        };
        let group = group
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" - ");

        draw_buckets_line(&mut image, &group, buckets, cfg);
    }

    let name = format!("{}_{}.png", figure_name, metric);

    let path = experiment.path.join("figures").join(name);
    tracing::info!("Saving image to: {}", path.display());
    image.save(path).expect("Failed to save image");

    // Draw the legend

    let mut image = RgbImage::new(width as u32, height as u32);

    draw_filled_rect_mut(
        &mut image,
        Rect::at(0, 0).of_size(width as u32, height as u32),
        Rgb([255, 255, 255]),
    );

    for (i, group) in groups.iter().enumerate() {
        tracing::trace!("Processing group {i}: {:?}", group);
        if buckets[0] != 0.0 {
            buckets.insert(0, 0.0);
        }
        if buckets.last() != Some(&f64::INFINITY) {
            buckets.push(f64::INFINITY);
        }

        // create buckets between b[0-1], b[1-2], ..., b[n-1-n]
        let buckets: Vec<((f64, f64), Vec<f64>)> = buckets
            .windows(2)
            .map(|w| ((w[0], w[1]), vec![0.0]))
            .collect::<Vec<_>>();

        let cfg = BucketDrawConfig {
            width,
            height,
            margin,
            bucket_height,
            bucket_index: i,
            legend: true,
            fill_color: colors[i % colors.len()],
            hatched: hatched_indices.contains(&i),
        };
        let group = group
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" - ");

        draw_buckets_line(&mut image, &group, buckets, cfg);
    }

    let name = format!("{}_{}_legend.png", figure_name, metric);
    let path = experiment.path.join("figures").join(name);
    tracing::info!("Saving legend image to: {}", path.display());
    image.save(path).expect("Failed to save image");

    Ok(())
}

pub struct BucketDrawConfig {
    pub width: f64,
    pub height: f64,
    pub margin: f64,
    pub bucket_height: f64,
    pub bucket_index: usize,
    pub legend: bool,
    pub fill_color: Rgb<u8>,
    pub hatched: bool,
}

fn luma(r: u8, g: u8, b: u8) -> f64 {
    0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64
}

fn text_color(fill_color: Rgb<u8>) -> Rgb<u8> {
    // Use a simple heuristic to determine text color based on the fill color
    // If the fill color is light, use dark text; if dark, use light text
    let luma_value = luma(fill_color[0], fill_color[1], fill_color[2]);

    if luma_value > 100.0 {
        Rgb([0u8, 0u8, 0u8]) // Dark text for light background
    } else {
        Rgb([255u8, 255u8, 255u8]) // Light text for dark background
    }
}

fn rendered_text_width_and_height(text: &str, font: &FontRef, font_size: f64) -> (f64, f64) {
    let scaled_font = font.as_scaled(font_size as f32);
    let mut width = 0.0;

    for ch in text.chars() {
        let glyph = scaled_font.scaled_glyph(ch);
        width += scaled_font.h_advance(glyph.id);
    }

    let height = scaled_font.height() as f64;

    (width as f64, height)
}

/// Draw diagonal hatch lines over a rectangular region
#[allow(clippy::too_many_arguments)]
fn draw_hatch_lines(
    image: &mut RgbImage,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    line_color: Rgb<u8>,
    spacing: i32,
    line_width: i32,
) {
    // Draw diagonal lines from bottom-left to top-right
    let x_end = x + width as i32;
    let y_end = y + height as i32;

    // Start from the left side and bottom side
    let mut start_x = x - height as i32;
    while start_x < x_end {
        // Calculate line endpoints clipped to the rectangle
        let x1 = start_x.max(x);
        let y1 = (y_end - (x1 - start_x)).min(y_end).max(y);
        let x2 = (start_x + height as i32).min(x_end);
        let y2 = (y_end - (x2 - start_x)).min(y_end).max(y);

        // Draw the line with thickness
        for offset in 0..line_width {
            imageproc::drawing::draw_line_segment_mut(
                image,
                ((x1 + offset) as f32, y1 as f32),
                ((x2 + offset) as f32, y2 as f32),
                line_color,
            );
        }

        start_x += spacing;
    }
}

fn draw_buckets_line(
    image: &mut RgbImage,
    _group_label: &str,
    buckets: Vec<((f64, f64), Vec<f64>)>,
    mut cfg: BucketDrawConfig,
) {
    let total_num_bugs = buckets
        .iter()
        .map(|(_, metrics)| metrics.len())
        .sum::<usize>();

    let single_bug_width = (cfg.width - 2.0 * cfg.margin) / total_num_bugs as f64;

    let mut x = cfg.margin;
    let y = cfg.margin + cfg.bucket_index as f64 * (cfg.bucket_height + cfg.margin);

    let scale = if cfg.legend {
        cfg.bucket_height * 0.2
    } else {
        cfg.bucket_height * 0.4
    };

    let color_moves = [
        (240u8 - cfg.fill_color[0]) / (buckets.len() - 1) as u8,
        (240u8 - cfg.fill_color[1]) / (buckets.len() - 1) as u8,
        (240u8 - cfg.fill_color[2]) / (buckets.len() - 1) as u8,
    ];

    let font = ab_glyph::FontRef::try_from_slice(include_bytes!(
        "../../../assets/SourceCodePro-Medium.ttf"
    ))
    .expect("Failed to load font");

    // Draw the group label at the top of the line
    // let (text_width, text_height) = rendered_text_width_and_height(group_label, &font, scale);
    // let text_x = cfg.margin + (cfg.width - 2.0 * cfg.margin) / 2.0 - text_width / 2.0;
    // let text_y = y - text_height - cfg.margin;
    // tracing::trace!(
    //     "Drawing group label '{}' at ({}, {}) with color {:?}",
    //     group_label,
    //     text_x,
    //     text_y,
    //     text_color(cfg.fill_color)
    // );
    // imageproc::drawing::draw_text_mut(
    //     image,
    //     text_color(cfg.fill_color),
    //     text_x as i32,
    //     text_y as i32,
    //     scale as f32,
    //     &font,
    //     group_label,
    // );

    for ((begin, end), metrics) in buckets {
        if metrics.is_empty() {
            cfg.fill_color[0] = cfg.fill_color[0].saturating_add(color_moves[0]);
            cfg.fill_color[1] = cfg.fill_color[1].saturating_add(color_moves[1]);
            cfg.fill_color[2] = cfg.fill_color[2].saturating_add(color_moves[2]);

            continue; // Skip empty buckets
        }

        // Calculate the width of the bucket based on the number of metrics
        let bucket_width = metrics.len() as f64 * single_bug_width;

        // Calculate the text color based on the fill color
        let text_color = text_color(cfg.fill_color);

        tracing::trace!(
            "Filling rectangle from ({}, {}) to ({}, {}) with color {:?}",
            x,
            y,
            x + bucket_width,
            y + cfg.bucket_height,
            cfg.fill_color
        );

        let rect =
            Rect::at(x as i32, y as i32).of_size(bucket_width as u32, cfg.bucket_height as u32);

        draw_filled_rect_mut(image, rect, cfg.fill_color);

        // Draw hatch lines if enabled
        if cfg.hatched {
            let hatch_color = Rgb([255u8, 255u8, 255u8]); // White hatching
            let spacing = (cfg.bucket_height * 0.15).max(8.0) as i32;
            let line_width = (cfg.bucket_height * 0.04).max(2.0) as i32;
            draw_hatch_lines(
                image,
                x as i32,
                y as i32,
                bucket_width as u32,
                cfg.bucket_height as u32,
                hatch_color,
                spacing,
                line_width,
            );
        }

        // Write the bucket label
        let label = if cfg.legend {
            format!("{} - {}", begin, end)
        } else {
            format!("{}", metrics.len())
        };
        let (text_width, text_height) = rendered_text_width_and_height(&label, &font, scale);

        if text_width > bucket_width {
            tracing::trace!(
                "Text width ({}) exceeds bucket width ({}), not drawing text",
                text_width,
                bucket_width
            );
        } else {
            let text_x = x + (bucket_width / 2.0) - (text_width / 2.0);
            let text_y = y + (cfg.bucket_height / 2.0) - (text_height / 2.0);
            tracing::trace!(
                "Drawing text '{}' at ({}, {}) with color {:?}",
                label,
                text_x,
                text_y,
                text_color
            );

            // Draw solid background behind text for readability on hatched bars
            if cfg.hatched {
                let padding = scale * 0.2;
                let bg_x = text_x - padding;
                let bg_y = text_y - padding;
                let bg_width = text_width + 2.0 * padding;
                let bg_height = text_height + 2.0 * padding;

                draw_filled_rect_mut(
                    image,
                    Rect::at(bg_x as i32, bg_y as i32).of_size(bg_width as u32, bg_height as u32),
                    cfg.fill_color,
                );
            }

            imageproc::drawing::draw_text_mut(
                image,
                text_color,
                text_x as i32,
                text_y as i32,
                scale as f32,
                &font,
                &label,
            );
        }
        cfg.fill_color[0] = cfg.fill_color[0].saturating_add(color_moves[0]);
        cfg.fill_color[1] = cfg.fill_color[1].saturating_add(color_moves[1]);
        cfg.fill_color[2] = cfg.fill_color[2].saturating_add(color_moves[2]);

        x += bucket_width;
    }
}

/// Draw bar charts for the given data.
/// The bars will show the total of a metric, aggregated
#[allow(clippy::too_many_arguments)]
pub fn draw_bar_chart(
    experiment: &ExperimentMetadata,
    figure_name: String,
    agg_metrics: Vec<Map<String, Value>>,
    groupby: Vec<String>,
    _group: AggregationType,
    _agg: AggregationType,
    metric: MetricType,
    max: Option<f64>,
) -> anyhow::Result<()> {
    tracing::trace!("Drawing bar chart for experiment '{:?}'", experiment.name);

    let mut groups = groupby
        .iter()
        .map(|g| {
            agg_metrics
                .iter()
                .map(|m| {
                    m.get(g)
                        .unwrap_or_else(|| panic!("Groupby field '{g}' not found"))
                })
                .unique()
                .collect::<Vec<_>>()
        })
        .multi_cartesian_product()
        .collect::<Vec<Vec<_>>>();

    // Remove empty groups, meaning groups that filter no metrics.
    groups.retain(|g| {
        agg_metrics.iter().any(|m| {
            groupby
                .iter()
                .enumerate()
                .all(|(i, g_)| m.get(g_).is_some_and(|v| g[i] == v))
        })
    });

    // The following code creates the sizes of the image;
    // Image width is fixed at 4000px.
    let height = 2000.0;
    // The ratio of the height is calculated based on the number of groups.
    let ratio = 1.0 * (0.99_f64.powi(groups.len() as i32));
    let width = (height / ratio).round();
    // The width of each bar is calculated based on the number of groups.
    // The margins between the bars are 10% of the bar width.
    // For N groups, the width of the image is;
    // width  = N * bar_width + 0.5 * bar_width.
    // --> bar_width = width / (N + 0.5).
    let bar_width = width / (groups.len() as f64 + 0.5);
    // The margin between the bars is 10% of the bar width.
    let hmargin = bar_width * 0.1;
    let vmargin = bar_width * 0.5;

    let mut image = RgbImage::new(width as u32, height as u32);

    draw_filled_rect_mut(
        &mut image,
        Rect::at(0, 0).of_size(width as u32, height as u32),
        Rgb([255, 255, 255]),
    );

    let colors = [
        Rgb([0x00, 0x00, 0x00]), // black
        Rgb([0x90, 0x0D, 0x0D]), // red
        Rgb([0xDC, 0x5F, 0x00]), // orange
        Rgb([0x24, 0x37, 0x63]), // blue
        Rgb([0x43, 0x6E, 0x4F]), // green
        Rgb([0x47, 0x09, 0x38]), // purple
        Rgb([0xD6, 0x1C, 0x4E]), // pink
        Rgb([0x33, 0x47, 0x56]), // dark blue
        Rgb([0x29, 0x00, 0x01]), // dark brown
        Rgb([0x00, 0x00, 0x00]), // black
    ];

    let max = if let Some(max_value) = max {
        tracing::trace!("Max value: {}", max_value);
        max_value
    } else {
        let mut max = 0.0;
        for group in groups.iter() {
            let group_metrics = agg_metrics
                .iter()
                .filter(|m| {
                    groupby
                        .iter()
                        .enumerate()
                        .all(|(i, g)| m.get(g).is_some_and(|v| group[i] == v))
                })
                .collect::<Vec<_>>();

            // get total of the metric for the group
            let total = group_metrics.iter().fold(0.0, |acc, m| {
                acc + m
                    .get(metric.to_string().as_str())
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.0)
            });

            if total > max {
                max = total;
            }
        }
        max
    };

    // Draw the vertical line at the left side of the image
    let line_color = Rgb([0x00, 0x00, 0x00]); // black
    let line_rect = Rect::at(vmargin as i32, 0).of_size(20, (height - hmargin) as u32);
    draw_filled_rect_mut(&mut image, line_rect, line_color);

    // Draw the horizontal line at the bottom of the image
    let horizontal_line_rect =
        Rect::at(vmargin as i32, (height - hmargin) as i32).of_size(width as u32, 20);
    draw_filled_rect_mut(&mut image, horizontal_line_rect, line_color);

    // Tick marks on the vertical line
    let tick_count = 10;
    // get the order of magnitude of the max value
    let order = max.log10().ceil();
    // find the closest 10th value to the max
    let closest_tenth = (max / 10f64.powf(order - 1.0)).ceil() * 10f64.powf(order - 1.0);
    let tick_step = closest_tenth / tick_count as f64;
    tracing::trace!(
        "Max value: {}, Order: {}, Closest tenth: {}, Tick step: {}",
        max,
        order,
        closest_tenth,
        tick_step
    );
    // Draw tick marks and labels on the vertical line
    for i in 0..=tick_count {
        tracing::trace!(
            "Drawing tick mark for value {} at index {}",
            i as f64 * tick_step,
            i
        );
        let tick_value = i as f64 * tick_step;
        let tick_y = height - hmargin - (tick_value / max) * (height - 2.0 * vmargin);
        tracing::trace!(
            "Tick value: {}, Tick X:{}, Tick Y: {}",
            tick_value,
            (vmargin * 0.75) as i32,
            tick_y
        );
        let tick_rect =
            Rect::at((vmargin * 0.75) as i32, tick_y as i32).of_size((vmargin * 0.25) as u32, 20);
        draw_filled_rect_mut(&mut image, tick_rect, line_color);
        // Draw the tick label
        let label = format!("{:.2}", tick_value);
        let font = ab_glyph::FontRef::try_from_slice(include_bytes!(
            "../../../assets/SourceCodePro-Medium.ttf"
        ))
        .expect("Failed to load font");
        let scale = bar_width * 0.15;
        let (text_width, text_height) = rendered_text_width_and_height(&label, &font, scale);
        let text_x = vmargin * 0.75 - text_width - 5.0; // 5px padding
        let text_y = tick_y - text_height / 3.0; // Center the text vertically
        tracing::trace!(
            "Drawing tick label '{}' at ({}, {}) with color {:?}",
            label,
            text_x,
            text_y,
            line_color
        );
        imageproc::drawing::draw_text_mut(
            &mut image,
            line_color,
            text_x as i32,
            text_y as i32,
            scale as f32,
            &font,
            &label,
        );
    }

    for (i, group) in groups.iter().enumerate() {
        tracing::trace!("Processing group {i}: {:?}", group);
        let group_metrics = agg_metrics
            .iter()
            .filter(|m| {
                groupby
                    .iter()
                    .enumerate()
                    .all(|(i, g)| m.get(g).is_some_and(|v| group[i] == v))
            })
            .collect::<Vec<_>>();

        // get total of the metric for the group
        let total = group_metrics.iter().fold(0.0, |acc, m| {
            acc + m
                .get(metric.to_string().as_str())
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0)
        });

        tracing::trace!("Total for group {i}: {}", total);
        if total == 0.0 {
            tracing::warn!("Total for group {i} is 0, skipping");
            continue; // Skip groups with no data
        }
        // let bar_width = (width - 2.0 * hmargin) / groups.len() as f64;
        let bar_height = (total / max) * (height - 2.0 * hmargin);
        let x = vmargin + i as f64 * bar_width;
        let y = height - hmargin - bar_height;
        tracing::debug!(
            "Drawing bar for group {i} at ({}, {}) with size ({}, {}) and color {:?}",
            x,
            y,
            bar_width,
            bar_height,
            colors[i % colors.len()]
        );
        if bar_height < 1.0 {
            tracing::warn!(
                "Bar height for group {i} is less than 1px, skipping drawing to avoid artifacts"
            );
            continue; // Skip drawing bars that are too small
        }
        let rect = Rect::at(x as i32, y as i32).of_size(bar_width as u32, bar_height as u32);
        tracing::debug!(
            "Drawing rectangle for group {i} at ({}, {}) with size ({}, {}) and color {:?}",
            x,
            y,
            bar_width,
            bar_height,
            colors[i % colors.len()]
        );
        draw_filled_rect_mut(&mut image, rect, colors[i % colors.len()]);
    }

    let name = format!("{}_{}.png", figure_name, metric);

    let path = experiment.path.join("figures").join(name);
    tracing::info!("Saving image to: {}", path.display());
    image.save(path).expect("Failed to save image");

    Ok(())
}

/// JSON format for pre-computed bucket chart data
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketChartJson {
    pub num_buckets: usize,
    pub chart_names: Vec<String>,
    pub chart_colors: Vec<String>,
    pub bar_styles: Vec<String>,
    pub bucket_values: Vec<Vec<String>>,
}

/// Parse a hex color string (e.g., "#6d0e56") to Rgb<u8>
fn parse_hex_color(hex: &str) -> Rgb<u8> {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
    Rgb([r, g, b])
}

/// Draw a bucket chart from a pre-computed JSON file
pub fn draw_bucket_chart_from_json(input_path: &Path, output_path: &Path) -> anyhow::Result<()> {
    // Read and parse JSON file
    let json_content = std::fs::read_to_string(input_path)
        .with_context(|| format!("Failed to read JSON file at {}", input_path.display()))?;
    let chart_data: BucketChartJson = serde_json::from_str(&json_content)
        .with_context(|| format!("Failed to parse JSON file at {}", input_path.display()))?;

    let num_rows = chart_data.bucket_values.len();

    // Calculate image dimensions based on number of rows
    let width = 4000.0;
    let ratio = 0.2 + 0.1 * (num_rows as f64);
    let height = (width * ratio).round();
    let bucket_height = height / (num_rows as f64 * 1.1 + 0.1);
    let margin = bucket_height * 0.1;

    let mut image = RgbImage::new(width as u32, height as u32);

    // Fill with white background
    draw_filled_rect_mut(
        &mut image,
        Rect::at(0, 0).of_size(width as u32, height as u32),
        Rgb([255, 255, 255]),
    );

    // Draw each row
    for (i, bucket_values) in chart_data.bucket_values.iter().enumerate() {
        // Parse the color for this row
        let base_color = if i < chart_data.chart_colors.len() {
            parse_hex_color(&chart_data.chart_colors[i])
        } else {
            Rgb([0x00, 0x00, 0x00]) // Default to black
        };

        // Check if this row should be hatched
        let hatched = if i < chart_data.bar_styles.len() {
            chart_data.bar_styles[i] == "hatched"
        } else {
            false
        };

        // Convert bucket values to the format expected by draw_buckets_line
        // Each bucket count becomes a vec with that many dummy elements
        let buckets: Vec<((f64, f64), Vec<f64>)> = bucket_values
            .iter()
            .enumerate()
            .map(|(bucket_idx, count_str)| {
                let count: usize = count_str.parse().unwrap_or(0);
                // Create dummy bucket bounds
                let start = bucket_idx as f64;
                let end = (bucket_idx + 1) as f64;
                // Create a vec with `count` elements (the actual values don't matter,
                // only the length is used for bucket width calculation)
                ((start, end), vec![1.0; count])
            })
            .collect();

        let cfg = BucketDrawConfig {
            width,
            height,
            margin,
            bucket_height,
            bucket_index: i,
            fill_color: base_color,
            legend: false,
            hatched,
        };

        let label = if i < chart_data.chart_names.len() {
            &chart_data.chart_names[i]
        } else {
            ""
        };

        draw_buckets_line(&mut image, label, buckets, cfg);
    }

    // Save the image
    tracing::info!("Saving image to: {}", output_path.display());
    image
        .save(output_path)
        .with_context(|| format!("Failed to save image to {}", output_path.display()))?;

    Ok(())
}
