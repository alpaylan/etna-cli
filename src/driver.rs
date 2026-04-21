use std::{
    collections::HashMap,
    io::Write as _,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex, RwLock},
};

use marauders::CustomLanguage;
use rayon::iter::{IntoParallelRefIterator as _, ParallelIterator as _};
use serde_json::{Map, Value};
use std::time::Duration;

use crate::{
    git_driver,
    manager::Manager,
    open_pbt_format::Status,
    store::{Metric, Store},
    workload::{Capability, Command, Step, Steps, Workload, WorkloadManifest},
};

use process_control::{ChildExt, Control};

use crate::error_context::Context;
use crate::experiment::{CexSource, ExperimentMetadata, InputSource, Mode, Target, Test};

type Object = Map<String, Value>;

#[derive(Debug)]
pub(crate) struct RunConfig {
    pub(crate) experiment_name: String,
    pub(crate) experiment_hash: String,
    /// Primary target. For Cross, this is the consumer's (language, workload).
    pub(crate) language: String,
    pub(crate) workload: String,
    pub(crate) workload_dir: PathBuf,
    pub(crate) mutations: Vec<String>,
    pub(crate) task: HashMap<String, String>,
    pub(crate) trials: usize,
    pub(crate) timeout: f64,
    pub(crate) short_circuit: bool,
    pub(crate) parallel: bool,
    #[allow(dead_code)]
    pub(crate) seed: Option<u64>,
    pub(crate) mode: Mode,
    /// For Cross mode only: producer target + its on-disk path. None for other modes.
    pub(crate) producer: Option<TargetPath>,
    /// For Cross mode only: the consumer's `test` capability steps and tags.
    /// Realized per-batch with the input filepath injected as `${inputs}`.
    pub(crate) consumer_test: Option<ConsumerSteps>,
}

#[derive(Debug, Clone)]
pub(crate) struct TargetPath {
    pub(crate) target: Target,
    /// Producer's language resolved at load time (from its `etna.toml`).
    /// Used for metric tagging; the filter only matches on workload.
    pub(crate) language: String,
    pub(crate) dir: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct ConsumerSteps {
    pub(crate) steps: Vec<Step>,
    pub(crate) tags: HashMap<String, Vec<String>>,
}

pub(crate) fn load_workload(
    experiment: &ExperimentMetadata,
    workload: &str,
) -> anyhow::Result<Workload> {
    let workload_path = experiment.workload_path(workload).ok_or_else(|| {
        anyhow::anyhow!(
            "Workload '{}' not found under '{}/workloads'",
            workload,
            experiment.path.display()
        )
    })?;

    let steps_path = workload_path.join("steps.json");
    let steps_json: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&steps_path)
            .with_context(|| format!("could not read steps at '{}'", steps_path.display()))?,
    )
    .with_context(|| format!("steps file at '{}' is invalid", steps_path.display()))?;
    let steps = Steps::from_value(&steps_json)
        .with_context(|| format!("failed to load steps from '{}'", steps_path.display()))?;

    let manifest = WorkloadManifest::read(&workload_path)?;

    Ok(Workload {
        name: manifest.name,
        language: manifest.language,
        dir: workload_path,
        properties: vec![],
        variations: vec![],
        strategies: vec![],
        steps,
    })
}

/// Non-string entries (e.g. `witnesses`) are metadata, not template params.
fn task_to_strings(task: &HashMap<String, serde_json::Value>) -> HashMap<String, String> {
    task.iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
        .collect()
}

fn metric_matches<'a>(
    m: &'a Metric,
    language: Option<&str>,
    workload: Option<&str>,
    mutations: Option<&Vec<String>>,
    task: &HashMap<String, String>,
    timeout: Option<f64>,
    trial: Option<usize>,
    mode: Option<&str>,
    producer: Option<&Target>,
) -> Option<&'a Metric> {
    let language_match = language.is_none_or(|l| {
        m.data
            .get("language")
            .is_some_and(|v| v.as_str() == Some(l))
    });
    let workload_match = workload.is_none_or(|w| {
        m.data
            .get("workload")
            .is_some_and(|v| v.as_str() == Some(w))
    });
    let mutations_match = mutations.is_none_or(|muts| {
        m.data.get("mutations").is_some_and(|v| {
            v.as_array().is_some_and(|arr| {
                arr.iter()
                    .all(|mv| muts.contains(&mv.as_str().unwrap().to_string()))
            })
        })
    });

    let task_match = task.iter().all(|(k, v)| {
        m.data
            .get(k)
            .is_some_and(|val| val.as_str() == Some(v.as_str()))
    });

    let timeout_match = timeout.is_none_or(|t| {
        m.data
            .get("timeout")
            .and_then(|v| v.as_f64())
            .is_some_and(|v| v >= t)
    });
    let trial_match = trial.is_none_or(|t| {
        m.data
            .get("trial")
            .and_then(|v| v.as_u64())
            .is_some_and(|v| v as usize == t)
    });
    let mode_match =
        mode.is_none_or(|name| m.data.get("mode").and_then(|v| v.as_str()) == Some(name));
    let producer_match = producer.is_none_or(|p| {
        m.data.get("producer_workload").and_then(|v| v.as_str()) == Some(p.workload.as_str())
    });

    if language_match
        && workload_match
        && mutations_match
        && task_match
        && timeout_match
        && trial_match
        && mode_match
        && producer_match
    {
        Some(m)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn task_completed(
    language: &str,
    workload: &str,
    mutations: &Vec<String>,
    task: &HashMap<String, String>,
    timeout: f64,
    trials: usize,
    short_circuit: bool,
    mode_name: &str,
    producer: Option<&Target>,
    metrics: &[Metric],
) -> bool {
    tracing::debug!(
        "Checking if task is completed for language '{}', workload '{}', mutations '{:?}', task '{:?}'",
        language, workload, mutations, task
    );
    let filtered_metrics = metrics
        .iter()
        .filter(|m| {
            metric_matches(
                m,
                Some(language),
                Some(workload),
                Some(mutations),
                task,
                None,
                None,
                Some(mode_name),
                producer,
            )
            .is_some()
        })
        .collect::<Vec<_>>();
    tracing::debug!(
        "Found {} matching metrics for the task",
        filtered_metrics.len()
    );

    let mut timed_out = false;
    (0..trials as u64).all(|i| {
        filtered_metrics
            .iter()
            .find(|m| m.data.get("trial").and_then(|v| v.as_u64()).map(|u| u == i) == Some(true))
            .and_then(|m| {
                tracing::trace!("Checking metric: {:?} for trial {}", m.data, i);
                if short_circuit {
                    if timed_out {
                        return Some(true);
                    }
                    if m.data
                        .get("status")
                        .is_some_and(|v| v.as_str() == Some(Status::TimedOut.to_string().as_str()))
                    {
                        timed_out = true;
                    }
                }
                m.data
                    .get("timeout")
                    .and_then(|v| v.as_f64())
                    .map(|t| t >= timeout)
            })
            == Some(true)
            || timed_out
    })
}

pub(crate) fn run(
    mgr: Arc<Mutex<Manager>>,
    run_config: &RunConfig,
    test_steps: &[Step],
    params: &mut HashMap<String, String>,
    tags: &HashMap<String, Vec<String>>,
    cancel_flag: Option<Arc<RwLock<bool>>>,
) -> anyhow::Result<()> {
    tracing::trace!("Running with config: {:?}", run_config);
    tracing::trace!("Run step: {:?}", test_steps);

    params.extend(run_config.task.clone());
    params.insert("language".to_string(), run_config.language.clone());
    params.insert("workload".to_string(), run_config.workload.clone());
    params.insert(
        "workload_path".to_string(),
        run_config.workload_dir.display().to_string(),
    );
    params.insert("mode".to_string(), run_config.mode.name().to_string());
    params.insert("timeout".to_string(), run_config.timeout.to_string());
    params.insert("mutations".to_string(), run_config.mutations.join(","));
    params.insert("experiment".to_string(), run_config.experiment_name.clone());
    params.insert("hash".to_string(), run_config.experiment_hash.clone());
    if let Some(prod) = &run_config.producer {
        params.insert("producer_language".to_string(), prod.language.clone());
        params.insert("producer_workload".to_string(), prod.target.workload.clone());
        params.insert(
            "producer_workload_path".to_string(),
            prod.dir.display().to_string(),
        );
    }

    tracing::trace!("Final params for step: {:?}", params);

    let test_steps = test_steps
        .iter()
        .map(|step| step.realize(params, tags))
        .collect::<Vec<_>>();

    anyhow::ensure!(test_steps.iter().all(anyhow::Result::is_ok));

    let test_steps = test_steps
        .into_iter()
        .flat_map(anyhow::Result::unwrap)
        .collect::<Vec<_>>();

    for step in &test_steps {
        tracing::trace!("Test step: {}", step);
    }

    let producer_target = run_config.producer.as_ref().map(|p| &p.target);

    let mut remaining_trials = vec![];
    {
        let mgr = mgr.lock().unwrap();
        let store = mgr.require_store()?;
        for i in 0..run_config.trials {
            let previous_metric = store.metrics.iter().find(|m| {
                metric_matches(
                    m,
                    Some(&run_config.language),
                    Some(&run_config.workload),
                    Some(&run_config.mutations),
                    &run_config.task,
                    Some(run_config.timeout),
                    Some(i),
                    Some(run_config.mode.name()),
                    producer_target,
                )
                .is_some()
            });

            if previous_metric.is_none() {
                remaining_trials.push(i);
            }
        }
    }
    if remaining_trials.is_empty() {
        tracing::info!(
            "All trials for the current task with language '{}', workload '{}' and mutations '{:?}', task '{:?}' are already completed, skipping the run steps.",
            run_config.language,
            run_config.workload,
            run_config.mutations,
            run_config.task
        );
        return Ok(());
    } else {
        tracing::info!(
            "{} out of {} trials are remaining for the current task with language '{}', workload '{}', mutations '{:?}', task '{:?}'.",
            remaining_trials.len(),
            run_config.trials,
            run_config.language,
            run_config.workload,
            run_config.mutations,
            run_config.task
        );
    }
    if run_config.parallel {
        run_remaining_trials_parallel(
            mgr,
            run_config,
            test_steps,
            remaining_trials,
            params,
            tags,
            cancel_flag,
        )
    } else {
        run_remaining_trials_sequential(
            mgr,
            run_config,
            test_steps,
            remaining_trials,
            params,
            tags,
            cancel_flag,
        )
    }
}

/// Build the per-trial metric context shared across all modes.
fn build_context(run_config: &RunConfig, trial: usize) -> Object {
    let mut ctx = serde_json::json!({
        "language": run_config.language,
        "workload": run_config.workload,
        "experiment": run_config.experiment_name,
        "mutations": run_config.mutations,
        "trial": trial,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "mode": run_config.mode.name(),
        "timeout": run_config.timeout,
    })
    .as_object()
    .unwrap()
    .to_owned();

    if let Some(prod) = &run_config.producer {
        ctx.insert(
            "producer_language".to_owned(),
            Value::String(prod.language.clone()),
        );
        ctx.insert(
            "producer_workload".to_owned(),
            Value::String(prod.target.workload.clone()),
        );
    }

    for (k, v) in &run_config.task {
        ctx.insert(k.to_owned(), Value::String(v.to_owned()));
    }

    ctx
}

/// Dispatch a single trial step to the appropriate per-mode runner.
fn dispatch_step(
    mgr: Arc<Mutex<Manager>>,
    run_config: &RunConfig,
    step: &Step,
    params: &HashMap<String, String>,
    tags: &HashMap<String, Vec<String>>,
    trial: usize,
) -> anyhow::Result<Status> {
    let realized = step.decide(params, tags);
    tracing::trace!("step '{step}' is evaluated to '{realized}' with params: {params:?}");

    let context = build_context(run_config, trial);

    let cmd = std::process::Command::from(&realized);
    match &run_config.mode {
        Mode::Cross { .. } => run_cross(mgr, context, cmd, &realized, run_config, params, tags),
        Mode::Solve | Mode::Sample { .. } | Mode::Test { .. } | Mode::Shrink { .. } => {
            run_subprocess(mgr, context, cmd, &realized, run_config)
        }
    }
}

fn run_remaining_trials_sequential(
    mgr: Arc<Mutex<Manager>>,
    run_config: &RunConfig,
    test_steps: Vec<Step>,
    remaining_trials: Vec<usize>,
    params: &mut HashMap<String, String>,
    tags: &HashMap<String, Vec<String>>,
    cancel_flag: Option<Arc<RwLock<bool>>>,
) -> anyhow::Result<()> {
    for i in remaining_trials {
        if let Some(ref flag) = cancel_flag {
            if *flag.read().unwrap() {
                tracing::info!("Job cancelled, stopping experiment");
                anyhow::bail!("Job cancelled");
            }
        }

        tracing::trace!("running trial {}", i);

        for step in &test_steps {
            let status = dispatch_step(mgr.clone(), run_config, step, params, tags, i)?;

            if status == Status::TimedOut {
                if run_config.short_circuit {
                    tracing::info!("Short-circuiting the experiment due to timeout");
                    return Ok(());
                } else {
                    tracing::info!("Process timed out, but short-circuit is not enabled, so continuing with the next trial");
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
struct EarlyStop;
impl std::fmt::Display for EarlyStop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("early stop requested")
    }
}
impl std::error::Error for EarlyStop {}

fn run_remaining_trials_parallel(
    mgr: Arc<Mutex<Manager>>,
    run_config: &RunConfig,
    test_steps: Vec<Step>,
    remaining_trials: Vec<usize>,
    params: &mut HashMap<String, String>,
    tags: &HashMap<String, Vec<String>>,
    cancel_flag: Option<Arc<RwLock<bool>>>,
) -> anyhow::Result<()> {
    tracing::trace!(
        "Running remaining trials in parallel: {:?}",
        remaining_trials
    );
    let short_circuit = run_config.short_circuit;

    remaining_trials
        .par_iter()
        .copied()
        .try_for_each(|i| -> anyhow::Result<()> {
            if let Some(ref flag) = cancel_flag {
                if *flag.read().unwrap() {
                    tracing::info!("Job cancelled, stopping experiment");
                    anyhow::bail!("Job cancelled");
                }
            }

            tracing::trace!("running trial {}", i);

            for step in &test_steps {
                let status = dispatch_step(mgr.clone(), run_config, step, params, tags, i)?;

                if status == Status::TimedOut {
                    if short_circuit {
                        tracing::info!("Short-circuiting the experiment due to timeout");
                        return Err(anyhow::anyhow!(EarlyStop));
                    } else {
                        tracing::info!(
                            "Process timed out, but short-circuit is not enabled; continuing"
                        );
                    }
                }
            }

            Ok(())
        })
        .map_err(|e| {
            if e.downcast_ref::<EarlyStop>().is_some() {
                anyhow::anyhow!("aborted remaining trials due to timeout short-circuit")
            } else {
                e
            }
        })?;

    Ok(())
}

/// Cross-mode runner: iterates `producer.sample` → `consumer.test` until timeout or FoundBug.
///
/// 1. Run the producer's sampler (passed in as `cmd`).
/// 2. Parse its stdout as a JSON array of `{time, value}` samples.
/// 3. Write the samples to a tempfile in s-expression form `(s1 s2 ...)`.
/// 4. Realize the consumer's `test` capability steps with `${inputs}` bound to the tempfile path.
/// 5. Run the consumer's test steps; the final step's stdout is the campaign-result JSON.
/// 6. Accumulate per-sample durations against the overall timeout; log FoundBug or keep going.
#[allow(clippy::too_many_arguments)]
fn run_cross(
    mgr: Arc<Mutex<Manager>>,
    mut context: Object,
    mut cmd: std::process::Command,
    step: &Command,
    run_config: &RunConfig,
    params: &HashMap<String, String>,
    _tags: &HashMap<String, Vec<String>>,
) -> anyhow::Result<Status> {
    let consumer = run_config
        .consumer_test
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("run_cross called without consumer_test in RunConfig"))?;

    let timeout = Duration::from_secs_f64(run_config.timeout);

    let mut total_time = Duration::default();
    let mut total_passed = 0;
    let mut total_discards = 0;
    let mut total_samples = 0;

    while total_time < timeout {
        // sample the command
        tracing::debug!("sampling command: {}", step);
        let child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                tracing::error!("Failed to spawn command '{}': {}", step, e);
                e
            })
            .with_context(|| format!("Failed to spawn '{}'", step))?
            .wait_with_output()
            .map_err(|e| {
                tracing::error!("Failed to run command '{}': {}", step, e);
                e
            })
            .with_context(|| format!("Failed to run command '{}'", step));

        match child {
            Ok(output) => {
                let logs = {
                    let mut mgr = mgr.lock().unwrap();
                    log_process_output(
                        &output,
                        mgr.require_store_mut()?,
                        &run_config.experiment_hash,
                        &context,
                    )?
                };

                if logs.is_empty() {
                    tracing::warn!("No logs collected from command '{}'", step);
                }
                for log in logs {
                    tracing::debug!("log: {:?}", log);
                }

                let stdout = String::from_utf8_lossy(&output.stdout);

                if !output.status.success() {
                    tracing::error!("Sampler '{}' failed with status: {}", step, output.status);
                    context.insert(
                        "status".to_owned(),
                        Value::String(Status::Aborted.to_string()),
                    );
                    context.insert(
                        "error".to_owned(),
                        Value::String(format!(
                            "sampler '{}' exited with status {}",
                            step, output.status
                        )),
                    );
                    let mut mgr = mgr.lock().unwrap();
                    mgr.require_store_mut()?.push(Metric {
                        data: context.clone(),
                        hash: run_config.experiment_hash.clone(),
                    })?;
                    return Ok(Status::Aborted);
                }

                let samples: Vec<serde_json::Value> = serde_json::from_str(&stdout)
                    .context(format!("Failed to parse output of command '{}'", step))?;

                let (durations, samples) = samples
                    .into_iter()
                    .map(|s| {
                        let duration = s.get("time").and_then(|v| v.as_str()).unwrap_or("unknown");
                        let sample = s.get("value").and_then(|v| v.as_str()).unwrap_or("unknown");
                        (duration.to_string(), sample.to_string())
                    })
                    .unzip::<String, String, Vec<_>, Vec<_>>();

                tracing::debug!("{} samples collected", samples.len());

                // write samples to a temporary file
                let mut temp_file =
                    tempfile::NamedTempFile::new().context("Failed to create temporary file")?;
                temp_file
                    .write_all(format!("({})", samples.join(" ")).as_bytes())
                    .context("Failed to write samples to temporary file")?;

                // Realize consumer's test steps with ${inputs} pointing at the tempfile.
                let mut cparams = params.clone();
                cparams.insert(
                    "inputs".to_string(),
                    temp_file.path().display().to_string(),
                );

                tracing::debug!(
                    "Running consumer test for workload '{}/{}' with inputs at {}",
                    run_config.language,
                    run_config.workload,
                    temp_file.path().display()
                );

                let results = run_consumer_test(&consumer.steps, &cparams, &consumer.tags);

                let Ok(results) = results else {
                    tracing::error!("Failed to run consumer test");
                    let err = results.unwrap_err();
                    tracing::error!("error: {}", err);
                    context.insert(
                        "status".to_owned(),
                        Value::String(Status::Aborted.to_string()),
                    );
                    context.insert("error".to_owned(), Value::String(err.to_string()));
                    let mut mgr = mgr.lock().unwrap();
                    mgr.require_store_mut()?.push(Metric {
                        data: context.clone(),
                        hash: run_config.experiment_hash.clone(),
                    })?;
                    return Ok(Status::Aborted);
                };

                let status = results
                    .get("status")
                    .and_then(|v| serde_json::from_value::<Status>(v.clone()).ok())
                    .context("Failed to get 'status' from consumer output")?;

                let passed = results.get("tests").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

                let discarded = results
                    .get("discarded")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as usize;

                let time_cutoff = if status == Status::FoundBug {
                    passed + discarded + 1
                } else {
                    passed + discarded
                };

                for (i, d) in durations.iter().take(time_cutoff).enumerate() {
                    let d = parse_duration::parse(d)
                        .with_context(|| format!("Failed to parse duration: {}", d))?;

                    total_time += d;

                    if total_time > timeout {
                        // these are approximate values, we divide by the time_cutoff to get the average
                        total_passed += passed * i / time_cutoff;
                        total_discards += discarded * i / time_cutoff;
                        total_samples += i;
                        tracing::info!("Timeout reached after {:?}, stopping the run", total_time);
                        break;
                    }
                }

                tracing::debug!(
                    "Total time for this batch: {:?}, total samples: {}",
                    total_time,
                    time_cutoff
                );
                if status == Status::FoundBug {
                    tracing::info!("Found a bug in the canonical serializer, stopping the run");

                    context.insert(
                        "status".to_owned(),
                        serde_json::Value::String(Status::FoundBug.to_string()),
                    );
                    context.insert(
                        "passed".to_owned(),
                        serde_json::Value::Number(total_passed.into()),
                    );
                    context.insert(
                        "discarded".to_owned(),
                        serde_json::Value::Number(total_discards.into()),
                    );
                    context.insert(
                        "tests".to_owned(),
                        serde_json::Value::Number((total_discards + total_passed + 1).into()),
                    );
                    context.insert(
                        "time".to_owned(),
                        serde_json::Value::String(format!("{}ns", total_time.as_nanos())),
                    );
                    context.insert(
                        "counterexample".to_owned(),
                        results
                            .get("counterexample")
                            .unwrap_or(&serde_json::Value::Null)
                            .clone(),
                    );
                    let mut mgr = mgr.lock().unwrap();
                    mgr.require_store_mut()?.push(Metric {
                        data: context.clone(),
                        hash: run_config.experiment_hash.clone(),
                    })?;

                    return Ok(Status::FoundBug);
                } else {
                    tracing::info!(
                        "No bugs found in this batch, continuing to the next batch if time allows"
                    );
                }
            }
            Err(err) => {
                tracing::error!("Failed to spawn child process: {}", err);
                context.insert(
                    "status".to_owned(),
                    serde_json::Value::String(Status::Aborted.to_string()),
                );
                context.insert("error".to_owned(), Value::String(err.to_string()));
                let mut mgr = mgr.lock().unwrap();
                mgr.require_store_mut()?.push(Metric {
                    data: context.clone(),
                    hash: run_config.experiment_hash.clone(),
                })?;

                return Ok(Status::Aborted);
            }
        }
    }

    tracing::info!(
        "Cross-language run completed in {:?} with {} samples",
        total_time,
        total_samples
    );
    context.insert(
        "status".to_owned(),
        serde_json::Value::String(Status::TimedOut.to_string()),
    );
    context.insert(
        "time".to_owned(),
        serde_json::Value::String(format!("{}ns", total_time.as_nanos())),
    );
    context.insert(
        "samples".to_owned(),
        serde_json::Value::Number(total_samples.into()),
    );

    let mut mgr = mgr.lock().unwrap();
    mgr.require_store_mut()?.push(Metric {
        data: context.clone(),
        hash: run_config.experiment_hash.clone(),
    })?;

    Ok(Status::TimedOut)
}

/// Realize the consumer's `test` capability steps (with `${inputs}` already set in `params`),
/// execute them in order, and return the final step's stdout parsed as a campaign-result JSON.
fn run_consumer_test(
    steps: &[Step],
    params: &HashMap<String, String>,
    tags: &HashMap<String, Vec<String>>,
) -> anyhow::Result<Object> {
    let realized: Vec<Step> = steps
        .iter()
        .map(|s| s.realize(params, tags))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();

    let mut last_stdout: Option<String> = None;
    for step in &realized {
        let decided = step.decide(params, tags);
        let mut cmd = std::process::Command::from(&decided);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        tracing::debug!("Running consumer step: {}", decided);
        let output = cmd
            .output()
            .with_context(|| format!("Failed to run consumer step '{}'", decided))?;
        if !output.status.success() {
            anyhow::bail!(
                "consumer step '{}' exited with status {}: {}",
                decided,
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        last_stdout = Some(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    let stdout = last_stdout
        .ok_or_else(|| anyhow::anyhow!("consumer test capability has no steps"))?;
    serde_json::from_str::<Object>(stdout.trim())
        .with_context(|| format!("Failed to parse consumer output as JSON: '{}'", stdout))
}

fn run_subprocess(
    mgr: Arc<Mutex<Manager>>,
    mut context: Object,
    mut cmd: std::process::Command,
    step: &Command,
    run_config: &RunConfig,
) -> anyhow::Result<Status> {
    tracing::debug!("Running command: {}", step);

    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to spawn '{}'", step))?
        .controlled_with_output()
        .time_limit(Duration::from_secs_f64(run_config.timeout))
        .terminate_for_timeout()
        .wait()
        .context(format!("Failed to run command '{}'", step));

    tracing::trace!("metadata: {:?}", context);

    match output {
        Ok(None) => {
            tracing::warn!("Process timed out after {} seconds", run_config.timeout);

            context.insert(
                "status".to_owned(),
                Value::String(Status::TimedOut.to_string()),
            );
            let mut mgr = mgr.lock().unwrap();
            mgr.require_store_mut()?.push(Metric {
                data: context.clone(),
                hash: run_config.experiment_hash.clone(),
            })?;

            Ok(Status::TimedOut)
        }
        Ok(Some(output)) => {
            if !output.status.success() {
                tracing::warn!("Command '{}' failed with status: {}", step, output.status);
            }
            let logs = {
                let mut mgr = mgr.lock().unwrap();
                log_process_output(
                    &output.into_std_lossy(),
                    mgr.require_store_mut()?,
                    &run_config.experiment_hash,
                    &context,
                )?
            };

            if logs.is_empty() {
                tracing::warn!("No logs collected from command '{}'", step);
            }
            for log in logs {
                tracing::debug!("log: {:?}", log);
            }

            Ok(Status::Unknown)
        }
        Err(err) => {
            tracing::error!("Aborting! Failed to run command '{}': {}", step, err);

            context.insert(
                "status".to_owned(),
                Value::String(Status::Aborted.to_string()),
            );
            context.insert("error".to_owned(), Value::String(err.to_string()));

            let mut mgr = mgr.lock().unwrap();
            mgr.require_store_mut()?.push(Metric {
                data: context.clone(),
                hash: run_config.experiment_hash.clone(),
            })?;

            Ok(Status::Aborted)
        }
    }
}

pub(crate) fn build(
    build_dir: &Path,
    check_steps: &[Step],
    build_steps: &[Step],
    params: &HashMap<String, String>,
    tags: &HashMap<String, Vec<String>>,
) -> anyhow::Result<()> {
    tracing::info!("running check commands...");
    let check_steps = check_steps
        .iter()
        .map(|step| step.realize(params, tags))
        .collect::<Vec<_>>();

    anyhow::ensure!(check_steps.iter().all(anyhow::Result::is_ok));

    let check_steps = check_steps
        .into_iter()
        .flat_map(anyhow::Result::unwrap)
        .collect::<Vec<_>>();

    for step in check_steps.iter() {
        tracing::debug!("running check step: {}", step);
        // Run the check command
        let step = step.decide(params, tags);
        tracing::debug!("step is evaluated to '{step}'");

        let mut cmd = std::process::Command::from(&step);
        cmd.current_dir(build_dir);

        let output = cmd.output().context("Failed to execute check command")?;

        if !output.status.success() {
            tracing::info!(
                "[✗] '{}' failed",
                step.command.clone() + " " + &step.args.join(" ")
            );
            anyhow::bail!("check command failed with status: {}", output.status);
        } else {
            tracing::info!(
                "[✓] '{}' passed",
                step.command.clone() + " " + &step.args.join(" ")
            );
        }
    }
    tracing::info!("check commands are successfull.");
    tracing::info!("running build commands...");

    let build_steps = build_steps
        .iter()
        .map(|step| step.realize(params, tags))
        .collect::<Vec<_>>();

    anyhow::ensure!(build_steps.iter().all(anyhow::Result::is_ok));

    let build_steps = build_steps
        .into_iter()
        .flat_map(anyhow::Result::unwrap)
        .collect::<Vec<_>>();

    for step in build_steps.iter() {
        // Run the build command
        tracing::debug!("running build step: {}", step);
        let step = step.decide(params, tags);
        tracing::debug!("step is evaluated to '{step}'");

        let mut cmd = std::process::Command::from(&step);
        cmd.current_dir(build_dir);

        let output = cmd
            .output()
            .inspect_err(|e| {
                tracing::error!("Failed to execute build command '{}': {}", step, e);
            })
            .with_context(|| format!("Failed to execute build command '{}'", step))?;

        if !output.status.success() {
            tracing::info!("[✗] '{}' failed", step);
            tracing::debug!("command: {}", step);
            tracing::debug!("stdout: {}", String::from_utf8_lossy(&output.stdout));
            anyhow::bail!(
                "build command failed with status: {}\nstderr: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        } else {
            tracing::info!("[✓] '{}' passed", step);
        }
    }
    tracing::info!("build commands are successfull.");

    Ok(())
}

pub(crate) fn run_experiment(
    mgr: Arc<Mutex<Manager>>,
    test: &Test,
    experiment: &ExperimentMetadata,
    short_circuit: bool,
    parallel: bool,
    cli_params: &HashMap<String, String>,
    cancel_flag: Option<Arc<RwLock<bool>>>,
) -> anyhow::Result<()> {
    tracing::info!(
        "Starting experiment '{}' with test: {:?}",
        experiment.name,
        test
    );
    // Snapshot the current version of the workload
    tracing::trace!("snapshotting current version of the workload...");
    git_driver::commit(
        &experiment.path,
        &format!("running experiment {} with test {}", experiment.name, test),
    )?;

    // If there is a local marauders configuration, use it.
    let custom_languages = if let Ok(project) = marauders::Project::new(&experiment.path, None) {
        tracing::trace!(
            "Using local marauders configuration at '{}'",
            experiment.path.display()
        );
        if let Some(cfg) = project.config {
            cfg.custom_languages.clone()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn aux(
        mgr: Arc<Mutex<Manager>>,
        test: &Test,
        experiment: &ExperimentMetadata,
        short_circuit: bool,
        parallel: bool,
        custom_languages: Vec<CustomLanguage>,
        cli_params: &HashMap<String, String>,
        cancel_flag: Option<Arc<RwLock<bool>>>,
    ) -> anyhow::Result<()> {
        // Primary target = where the property runs. For Cross this is the consumer; the
        // producer's sample capability is what we iterate each trial.
        let (primary_wl, producer_target): (String, Option<Target>) = match &test.mode {
            Mode::Cross { producer, consumer } => {
                (consumer.workload.clone(), Some(producer.clone()))
            }
            _ => (test.workload.clone(), None),
        };

        let workload: Workload = load_workload(experiment, &primary_wl)?;
        let primary_lang = workload.language.clone();
        let primary_dir = workload.dir.clone();

        // For Cross, also load the producer workload so we can pick its sample capability.
        let producer_workload: Option<Workload> = match &producer_target {
            Some(t) => Some(load_workload(experiment, &t.workload)?),
            None => None,
        };
        let producer_path: Option<TargetPath> = producer_workload
            .as_ref()
            .and_then(|pw| producer_target.as_ref().map(|t| TargetPath {
                target: t.clone(),
                language: pw.language.clone(),
                dir: pw.dir.clone(),
            }));

        // Apply marauders mutations to the primary target only.
        let lang = marauders::Language::name_to_language(&primary_lang, &custom_languages)
            .with_context(|| format!("language '{}' is not known or supported", primary_lang))?;
        let glob = format!("*.{}", lang.file_extension());
        let mut project = marauders::Project::new(&primary_dir, Some(&glob))?;
        marauders::reset_all(&mut project)?;
        for variant in test.mutations.iter() {
            marauders::set_variant(&mut project, variant)?;
        }

        // Pick the capability steps that will be iterated each trial, and the tags used for
        // template expansion. For Cross, iterate producer.sample and pass consumer.test as
        // `consumer_test` in RunConfig (realized per-batch inside run_cross).
        let (iter_steps, iter_tags, iter_dir): (Vec<Step>, HashMap<String, Vec<String>>, PathBuf) =
            match &test.mode {
                Mode::Solve => (
                    workload.steps.capability(Capability::Solve)?.clone(),
                    workload.steps.tags.clone(),
                    primary_dir.clone(),
                ),
                Mode::Sample { .. } => (
                    workload.steps.capability(Capability::Sample)?.clone(),
                    workload.steps.tags.clone(),
                    primary_dir.clone(),
                ),
                Mode::Test { .. } => (
                    workload.steps.capability(Capability::Test)?.clone(),
                    workload.steps.tags.clone(),
                    primary_dir.clone(),
                ),
                Mode::Shrink { .. } => (
                    workload.steps.capability(Capability::Shrink)?.clone(),
                    workload.steps.tags.clone(),
                    primary_dir.clone(),
                ),
                Mode::Cross { .. } => {
                    let pw = producer_workload.as_ref().unwrap();
                    let pp = producer_path.as_ref().unwrap();
                    (
                        pw.steps.capability(Capability::Sample)?.clone(),
                        pw.steps.tags.clone(),
                        pp.dir.clone(),
                    )
                }
            };

        let consumer_test = match &test.mode {
            Mode::Cross { .. } => Some(ConsumerSteps {
                steps: workload.steps.capability(Capability::Test)?.clone(),
                tags: workload.steps.tags.clone(),
            }),
            _ => None,
        };

        // Base params shared across all tasks.
        let mut base_params: HashMap<String, String> = HashMap::new();
        base_params.insert(
            "workload_path".to_string(),
            primary_dir.display().to_string(),
        );
        if let Some(pp) = &producer_path {
            base_params.insert(
                "producer_workload_path".to_string(),
                pp.dir.display().to_string(),
            );
        }

        if let Some(params_) = &test.params {
            for (key, value) in params_.iter() {
                tracing::trace!("Adding test parameter: {} = {}", key, value);
                base_params.insert(key.clone(), value.to_string());
            }
        }

        // CLI params override test.params (highest precedence)
        for (key, value) in cli_params.iter() {
            tracing::trace!("Applying CLI param: {} = {}", key, value);
            base_params.insert(key.clone(), value.clone());
        }

        // Resolve InputSource once for Test mode; keep the tempfile alive until we're done.
        let _input_tempfile: Option<tempfile::NamedTempFile> = match &test.mode {
            Mode::Test { inputs } => match inputs {
                InputSource::File(p) => {
                    base_params.insert("inputs".to_string(), p.display().to_string());
                    None
                }
                InputSource::Inline(items) => {
                    let mut tf = tempfile::NamedTempFile::new()
                        .context("Failed to create inputs tempfile")?;
                    let body = serde_json::to_string(items)
                        .context("Failed to serialize inline inputs")?;
                    tf.write_all(body.as_bytes())
                        .context("Failed to write inputs tempfile")?;
                    base_params
                        .insert("inputs".to_string(), tf.path().display().to_string());
                    Some(tf)
                }
            },
            _ => None,
        };

        // Resolve a non-FromTask counterexample once up-front. FromTask is resolved per-task below.
        let _cex_tempfile: Option<tempfile::NamedTempFile> = match &test.mode {
            Mode::Shrink { counterexample } => match counterexample {
                CexSource::Inline(s) => {
                    base_params.insert("counterexample".to_string(), s.clone());
                    None
                }
                CexSource::File(p) => {
                    let s = std::fs::read_to_string(p)
                        .with_context(|| format!("Failed to read counterexample file '{}'", p.display()))?;
                    base_params.insert("counterexample".to_string(), s.trim().to_string());
                    None
                }
                CexSource::FromTask => None,
            },
            _ => None,
        };

        tracing::trace!(
            "Checking if all tasks for language '{}' and workload '{}' are already completed",
            primary_lang,
            primary_wl
        );

        {
            let mgr = mgr.lock().unwrap();
            let store = mgr.require_store()?;
            let mode_name = test.mode.name();
            let all_tasks_completed = test.tasks.iter().all(|task| {
                let task_strings = task_to_strings(task);
                task_completed(
                    &primary_lang,
                    &primary_wl,
                    &test.mutations,
                    &task_strings,
                    test.timeout,
                    test.trials,
                    short_circuit,
                    mode_name,
                    producer_target.as_ref(),
                    &store.metrics,
                )
            });

            if all_tasks_completed {
                tracing::info!(
                    "All tasks for the current test with language '{}', workload '{}' and mutations '{:?}' are already completed, skipping the build and run steps.",
                    primary_lang,
                    primary_wl,
                    test.mutations
                );
                return Ok(());
            } else {
                tracing::info!(
                    "Not all tasks for the current test with language '{}', workload '{}' and mutations '{:?}' are completed, proceeding with the build and run steps.",
                    primary_lang,
                    primary_wl,
                    test.mutations
                );
            }
        }

        build(
            &primary_dir,
            &workload.steps.setup,
            &workload.steps.build,
            &base_params,
            &workload.steps.tags,
        )?;

        if let (Some(pw), Some(pp)) = (&producer_workload, &producer_path) {
            build(
                &pp.dir,
                &pw.steps.setup,
                &pw.steps.build,
                &base_params,
                &pw.steps.tags,
            )?;
        }

        let experiment_hash = experiment.hash()?;

        for task in test.tasks.iter() {
            // Check cancellation before each task
            if let Some(ref flag) = cancel_flag {
                if *flag.read().unwrap() {
                    tracing::info!("Job cancelled, stopping experiment");
                    anyhow::bail!("Job cancelled");
                }
            }

            let task_strings = task_to_strings(task);
            let mut params = base_params.clone();
            params.extend(task_strings.clone());

            // FromTask counterexample: pull from the task map.
            if let Mode::Shrink {
                counterexample: CexSource::FromTask,
            } = &test.mode
            {
                let cex = task_strings.get("counterexample").cloned().ok_or_else(|| {
                    anyhow::anyhow!(
                        "Shrink mode with CexSource::FromTask requires task to have a 'counterexample' field"
                    )
                })?;
                params.insert("counterexample".to_string(), cex);
            }

            let run_config = RunConfig {
                experiment_name: experiment.name.clone(),
                experiment_hash: experiment_hash.clone(),
                language: primary_lang.clone(),
                workload: primary_wl.clone(),
                workload_dir: primary_dir.clone(),
                mutations: test.mutations.clone(),
                task: task_strings,
                trials: test.trials,
                timeout: test.timeout,
                short_circuit,
                parallel,
                seed: None,
                mode: test.mode.clone(),
                producer: producer_path.clone(),
                consumer_test: consumer_test.clone(),
            };

            let result = run(
                mgr.clone(),
                &run_config,
                &iter_steps,
                &mut params,
                &iter_tags,
                cancel_flag.clone(),
            );

            if let Err(e) = &result {
                tracing::error!("Failed to run experiment: {}", e);
            }

            // Silence the unused-read warning for iter_dir.
            let _ = &iter_dir;
        }

        Ok(())
    }

    let result = aux(
        mgr,
        test,
        experiment,
        short_circuit,
        parallel,
        custom_languages,
        cli_params,
        cancel_flag,
    );
    if let Err(e) = &result {
        tracing::error!("Experiment failed with error: {}", e);
    } else {
        tracing::info!("Experiment completed successfully");
    }

    let mut project = marauders::Project::new(&experiment.path, None)?;
    marauders::reset_all(&mut project)?;
    result
}

fn log_process_output(
    output: &std::process::Output,
    store: &mut Store,
    experiment_hash: &str,
    context: &Object,
) -> anyhow::Result<Vec<Object>> {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    tracing::debug!("stdout: {}", stdout);
    tracing::debug!("stderr: {}", stderr);

    if !output.status.success() {
        tracing::error!("Process failed with status: {}", output.status);
        tracing::error!("stderr: {}", stderr);
        store.push(Metric {
            data: {
                let mut error_context = context.clone();
                // Drop task-specified docs counterexample; keep runtime data only.
                error_context.remove("counterexample");
                error_context.insert(
                    "status".to_owned(),
                    Value::String(Status::Aborted.to_string()),
                );
                error_context.insert(
                    "error".to_owned(),
                    Value::String(format!(
                        "Process failed with status: {}\nstderr: {}",
                        output.status, stderr
                    )),
                );
                error_context
            },
            hash: experiment_hash.to_string(),
        })?;
    }

    // Look for JSON objects in the output
    let mut logs = Vec::new();
    for line in stdout.lines().chain(stderr.lines()) {
        if let Ok(json) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(line) {
            tracing::info!("Found JSON object: {:?}", json);
            let mut merged = context.clone();
            // Drop task-specified docs counterexample; keep runtime data only.
            merged.remove("counterexample");
            // Runtime JSON should win over context defaults when keys overlap.
            merged.extend(json.clone());
            store.push(Metric {
                data: merged.clone(),
                hash: experiment_hash.to_string(),
            })?;
            logs.push(merged);
        }
    }

    Ok(logs)
}
