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
    workload::{Command, Language, Step, Steps, Workload},
};

use process_control::{ChildExt, Control};

use crate::experiment::{ExperimentMetadata, Test};
use crate::error_context::Context;

type Object = Map<String, Value>;

#[derive(Debug)]
pub(crate) struct RunConfig {
    pub(crate) experiment_name: String,
    pub(crate) experiment_hash: String,
    pub(crate) language: String,
    pub(crate) workload: String,
    pub(crate) workload_dir: PathBuf,
    pub(crate) mutations: Vec<String>,
    pub(crate) task: HashMap<String, String>,
    pub(crate) trials: usize,
    pub(crate) timeout: f64,
    pub(crate) short_circuit: bool,
    pub(crate) cross: bool,
    pub(crate) parallel: bool,
    #[allow(dead_code)]
    pub(crate) seed: Option<u64>,
}

fn load_language(experiment_path: &Path, language: &str) -> anyhow::Result<Language> {
    let language_path = experiment_path.join("workloads").join(language);
    let steps_path = language_path.join("steps.json");
    tracing::debug!("Loading language config from '{}'", steps_path.display());
    let steps: Steps = serde_json::from_str(
        &std::fs::read_to_string(&steps_path)
            .with_context(|| format!("could not read steps at '{}'", steps_path.display()))?,
    )
    .with_context(|| format!("steps file at '{}' is invalid", steps_path.display()))?;

    let language = Language {
        name: language.to_string(),
        steps,
    };

    Ok(language)
}

pub(crate) fn load_workload(
    experiment_path: &Path,
    language: &str,
    workload: &str,
) -> anyhow::Result<Workload> {
    let workload_path = experiment_path
        .join("workloads")
        .join(language)
        .join(workload);

    anyhow::ensure!(
        workload_path.exists(),
        "Workload directory not found at '{}'",
        workload_path.display()
    );

    let steps_path = workload_path.join("steps.json");

    let workload_steps: Option<serde_json::Value> = std::fs::read_to_string(steps_path)
        .map(|s| serde_json::from_str(&s).unwrap())
        .ok();

    let language = load_language(experiment_path, language)?;

    let steps = if let Some(workload_steps) = workload_steps {
        Steps::with_default(&workload_steps, &language.steps)
    } else {
        tracing::warn!(
            "No steps.json found for workload '{}', using language default steps",
            workload
        );
        language.steps.clone()
    };

    Ok(Workload {
        name: workload.to_string(),
        language: language.name,
        dir: workload_path,
        properties: vec![],
        variations: vec![],
        strategies: vec![],
        steps,
    })
}

#[allow(clippy::too_many_arguments)]
fn metric_matches<'a>(
    m: &'a Metric,
    language: Option<&str>,
    workload: Option<&str>,
    mutations: Option<&Vec<String>>,
    task: &HashMap<String, String>,
    timeout: Option<f64>,
    trial: Option<usize>,
    cross: Option<bool>,
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

    // Do not match on `counterexample`: it is advisory metadata from docs and
    // may differ from the strategy-produced runtime counterexample.
    let task_match = task
        .iter()
        .filter(|(k, _)| k.as_str() != "counterexample")
        .all(|(k, v)| {
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
    let cross_match =
        cross.is_none_or(|c| m.data.get("cross").and_then(|v| v.as_bool()) == Some(c));

    if language_match
        && workload_match
        && mutations_match
        && task_match
        && timeout_match
        && trial_match
        && cross_match
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
    cross: bool,
    metrics: &[Metric],
) -> bool {
    tracing::trace!(
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
                Some(cross),
            )
            .is_some()
        })
        .collect::<Vec<_>>();

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
    params.insert("cross".to_string(), run_config.cross.to_string());
    params.insert("timeout".to_string(), run_config.timeout.to_string());
    params.insert("mutations".to_string(), run_config.mutations.join(","));
    params.insert("language".to_string(), run_config.language.clone());
    params.insert("experiment".to_string(), run_config.experiment_name.clone());
    params.insert("hash".to_string(), run_config.experiment_hash.clone());

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

    let mut remaining_trials = vec![];
    {
        let mgr = mgr.lock().unwrap();
        for i in 0..run_config.trials {
            let previous_metric = mgr.store.metrics.iter().find(|m| {
                metric_matches(
                    m,
                    Some(&run_config.language),
                    Some(&run_config.workload),
                    Some(&run_config.mutations),
                    &run_config.task,
                    Some(run_config.timeout),
                    Some(i),
                    Some(run_config.cross),
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
        // Check cancellation before each trial
        if let Some(ref flag) = cancel_flag {
            if *flag.read().unwrap() {
                tracing::info!("Job cancelled, stopping experiment");
                anyhow::bail!("Job cancelled");
            }
        }

        tracing::trace!("running trial {}", i);

        for step in &test_steps {
            let old_step = step;
            let step = old_step.decide(params, tags);
            tracing::trace!("step '{old_step}' is evaluated to '{step}' with params: {params:?}");

            let cmd = std::process::Command::from(&step);

            let mut context = serde_json::json!({
                "language": run_config.language,
                "workload": run_config.workload,
                "experiment": run_config.experiment_name,
                "mutations": run_config.mutations,
                "trial": i,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "cross": run_config.cross,
                "timeout": run_config.timeout,
            })
            .as_object()
            .unwrap()
            .to_owned();

            for (k, v) in &run_config.task {
                context.insert(k.to_owned(), Value::String(v.to_owned()));
            }

            let status = if run_config.cross {
                tracing::debug!("Running cross-language command: {}", step);
                run_cross(mgr.clone(), context, cmd, &step, run_config)?
            } else {
                tracing::debug!("Running default command: {}", step);
                run_default(mgr.clone(), context, cmd, &step, run_config)?
            };

            // If the run timed out and short-circuit is enabled, break the loop.
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

    // Parallelize the outer loop.
    remaining_trials
        .par_iter()
        .copied()
        .try_for_each(|i| -> anyhow::Result<()> {
            // Check cancellation before each trial
            if let Some(ref flag) = cancel_flag {
                if *flag.read().unwrap() {
                    tracing::info!("Job cancelled, stopping experiment");
                    anyhow::bail!("Job cancelled");
                }
            }

            tracing::trace!("running trial {}", i);

            // Steps remain sequential inside a trial
            for step_tmpl in &test_steps {
                let old_step = step_tmpl;
                let step = old_step.decide(params, tags);
                tracing::trace!(
                    "step '{old_step}' is evaluated to '{step}' with params: {params:?}"
                );

                let cmd = std::process::Command::from(&step);

                // Build context fresh per step (as in your original code)
                let mut context = serde_json::json!({
                    "language": run_config.language,
                    "workload": run_config.workload,
                    "experiment": run_config.experiment_name,
                    "mutations": run_config.mutations,
                    "trial": i,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "cross": run_config.cross,
                    "timeout": run_config.timeout,
                })
                .as_object()
                .unwrap()
                .to_owned();

                for (k, v) in &run_config.task {
                    context.insert(k.to_owned(), serde_json::Value::String(v.to_owned()));
                }

                let status = if run_config.cross {
                    tracing::debug!("Running cross-language command: {}", step);
                    run_cross(mgr.clone(), context, cmd, &step, run_config)?
                } else {
                    tracing::debug!("Running default command: {}", step);
                    run_default(mgr.clone(), context, cmd, &step, run_config)?
                };

                if status == Status::TimedOut {
                    if short_circuit {
                        tracing::info!("Short-circuiting the experiment due to timeout");
                        // Convert to an error so try_for_each stops scheduling new trials.
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
            // Swallow EarlyStop into Ok(()) so the function returns success when short-circuit triggers.
            if e.downcast_ref::<EarlyStop>().is_some() {
                // We intentionally stopped early
                anyhow::anyhow!("aborted remaining trials due to timeout short-circuit")
            } else {
                e
            }
        })?;

    Ok(())
}

/// Runs the canonical serialized runner (Rust for now) for the given workload, mutation, property, and tests.
/// Report the index of the failing test if any.
fn run_canonical_serialized(
    mgr: Arc<Mutex<Manager>>,
    workload: &str,
    mutations: &[String],
    property: &str,
    tests: &str,
) -> anyhow::Result<Object> {
    // Change the current working directory to the workload directory
    let workload_dir = {
        let mgr = mgr.lock().unwrap();
        mgr.config
            .repo_dir()
            .join("workloads")
            .join("Rust")
            .join(workload)
    };

    // Run marauders to mutate the canonical serializer
    tracing::trace!(
        "Running marauders to mutate the canonical serializer for workload '{}', mutations '{:?}'",
        workload,
        mutations
    );
    let glob = format!("*.{}", marauders::Language::Rust.file_extension());
    let mut project = marauders::Project::new(&workload_dir, Some(&glob))?;
    marauders::reset_all(&mut project)?;

    for variant in mutations.iter() {
        tracing::trace!(
            "Running marauders to mutate the canonical serializer for workload '{}', variant '{}'",
            workload,
            variant
        );
        marauders::set_variant(&mut project, variant)?;
    }

    // Run the build command for the canonical serializer
    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(&workload_dir);
    cmd.args(["build", "--release"]);
    tracing::debug!(
        "running 'cargo build --release' in '{}'",
        workload_dir.display()
    );

    let output = cmd.output();
    match output {
        Ok(output) => {
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("Failed to build canonical serializer: '{}'", stderr);
            }
        }
        Err(e) => {
            anyhow::bail!("Failed to run 'cargo build --release': '{}'", e);
        }
    }

    // Run the canonical serializer
    tracing::debug!(
        "Running canonical serializer for workload '{}', mutations '{:?}', property '{}'",
        workload,
        mutations,
        property
    );
    let mut cmd = std::process::Command::new(
        PathBuf::from(&workload_dir)
            .join("target")
            .join("release")
            .join(format!("{}-serialized", workload.to_lowercase())),
    );
    cmd.args([tests, property]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    tracing::trace!("Running canonical serializer command: {:?}", cmd);
    let output = cmd.output();
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Parse the stdout to find the index of the failing test
            // The JSON output starts at [| and ends at |]
            tracing::trace!("Canonical serializer output: {}", stdout);

            let json_value: Object =
                serde_json::from_str(&stdout).context("Failed to parse JSON output")?;

            Ok(json_value)
        }
        Err(e) => {
            tracing::error!("Failed to run canonical serializer: {}", e);
            anyhow::bail!("Failed to run canonical serializer: '{}'", e);
        }
    }
}

/// Cross-language runner for comparing across different languages.
/// Logically runs the following;
/// 1. Runs `workloads/language/workload-sampler` for the given strategy and property.
/// 2. Parses the output of the sampler to get the samples along with their durations.
/// 3. Writes the samples to a temporary file.
/// 4. Mutates and builds `workloads/Rust/workload-serialized` for running the serializer.
/// 5. Runs `workloads/Rust/workload-serialized` with the temporary file and the property as arguments.
/// 6. Reads the output of the serializer from stderrr and puts it in the result store.
///
/// The function is designed to run in a loop until the timeout is reached, with batches of 1000 samples
/// collected in each iteration.
fn run_cross(
    mgr: Arc<Mutex<Manager>>,
    mut context: Object,
    mut cmd: std::process::Command,
    step: &Command,
    run_config: &RunConfig,
) -> anyhow::Result<Status> {
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
                        &mut mgr.store,
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
                    tracing::error!("Command '{}' failed with status: {}", step, output.status);
                    panic!("Command should not have a non-zero exit status when running in cross-language mode");
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

                // Call the Rust serializer for the specific workload
                tracing::debug!(
                    "Running canonical serializer for workload '{}', mutations '{:?}', property '{}'",
                    run_config.workload,
                    run_config.mutations,
                    run_config.task.get("property").unwrap_or(&"<unknown>".to_string())
                );
                tracing::debug!("Using temporary file: {}", temp_file.path().display());
                let results = run_canonical_serialized(
                    mgr.clone(),
                    &run_config.workload,
                    &run_config.mutations,
                    run_config
                        .task
                        .get("property")
                        .unwrap_or(&"<unknown>".to_string()),
                    temp_file.path().to_str().unwrap(),
                );

                let Ok(results) = results else {
                    tracing::error!("Failed to run canonical serializer");
                    tracing::error!("Results: {:?}", results);
                    context.insert(
                        "status".to_owned(),
                        Value::String(Status::Aborted.to_string()),
                    );

                    context.insert(
                        "error".to_owned(),
                        Value::String(results.unwrap_err().to_string()),
                    );
                    let mut mgr = mgr.lock().unwrap();
                    mgr.store.push(Metric {
                        data: context.clone(),
                        hash: run_config.experiment_hash.clone(),
                    })?;
                    return Ok(Status::Aborted);
                };

                let status = results
                    .get("status")
                    .and_then(|v| serde_json::from_value::<Status>(v.clone()).ok())
                    .context("Failed to get 'status' from canonical serializer output")?;

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
                    mgr.store.push(Metric {
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
                mgr.store.push(Metric {
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
    mgr.store.push(Metric {
        data: context.clone(),
        hash: run_config.experiment_hash.clone(),
    })?;

    Ok(Status::TimedOut)
}

fn run_default(
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
            mgr.store.push(Metric {
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
                    &mut mgr.store,
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
            mgr.store.push(Metric {
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
        let lang = marauders::Language::name_to_language(&test.language, &custom_languages)
            .with_context(|| format!("language '{}' is not known or supported", test.language))?;
        let glob = format!("*.{}", lang.file_extension());

        let workload_dir = experiment
            .path
            .join("workloads")
            .join(test.language.as_str())
            .join(test.workload.as_str());

        let mut project = marauders::Project::new(&workload_dir, Some(&glob))?;
        marauders::reset_all(&mut project)?;

        for variant in test.mutations.iter() {
            marauders::set_variant(&mut project, variant)?;
        }

        let workload: Workload = load_workload(
            &experiment.path,
            test.language.as_str(),
            test.workload.as_str(),
        )?;

        // todo: there's a bug when two params share a prefix, fix it.
        let mut params = HashMap::from([(
            "workload_path".to_string(),
            workload_dir.display().to_string(),
        )]);

        if let Some(params_) = &test.params {
            for (key, value) in params_.iter() {
                tracing::trace!("Adding test parameter: {} = {}", key, value);
                params.insert(key.clone(), value.to_string());
            }
        };

        // CLI params override test.params (highest precedence)
        for (key, value) in cli_params.iter() {
            tracing::trace!("Applying CLI param: {} = {}", key, value);
            params.insert(key.clone(), value.clone());
        }

        tracing::trace!(
            "Checking if all tasks for language '{}' and workload '{}' are already completed",
            test.language,
            test.workload
        );

        {
            let mgr = mgr.lock().unwrap();
            let all_tasks_completed = test.tasks.iter().all(|task| {
                task_completed(
                    &test.language,
                    &test.workload,
                    &test.mutations,
                    task,
                    test.timeout,
                    test.trials,
                    short_circuit,
                    test.cross,
                    &mgr.store.metrics,
                )
            });

            if all_tasks_completed {
                tracing::info!(
                    "All tasks for the current test with language '{}', workload '{}' and mutations '{:?}' are already completed, skipping the build and run steps.",
                    test.language,
                    test.workload,
                    test.mutations
                );
                return Ok(());
            }
        }

        build(
            &workload_dir,
            &workload.steps.setup,
            &workload.steps.build,
            &params,
            &workload.steps.tags,
        )?;

        for task in test.tasks.iter() {
            // Check cancellation before each task
            if let Some(ref flag) = cancel_flag {
                if *flag.read().unwrap() {
                    tracing::info!("Job cancelled, stopping experiment");
                    anyhow::bail!("Job cancelled");
                }
            }

            params.extend(task.clone());

            // Run the experiment
            let run_config = RunConfig {
                language: test.language.clone(),
                workload_dir: workload_dir.clone(),
                experiment_hash: experiment.hash()?,
                trials: test.trials,
                workload: test.workload.clone(),
                mutations: test.mutations.clone(),
                task: task.clone(),
                timeout: test.timeout,
                short_circuit,
                cross: test.cross,
                parallel,
                seed: None,
                experiment_name: experiment.name.clone(),
            };

            let result = run(
                mgr.clone(),
                &run_config,
                &workload.steps.test,
                &mut params,
                &workload.steps.tags,
                cancel_flag.clone(),
            );

            if let Err(e) = &result {
                tracing::error!("Failed to run experiment: {}", e);
            }
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
        if let Ok(json) =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(line)
        {
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
