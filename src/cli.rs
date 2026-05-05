use std::{env, path::PathBuf};

use clap::{CommandFactory, Parser, Subcommand};

/// Parse a key=value pair for CLI parameters
fn parse_key_value(s: &str) -> Result<(String, String), String> {
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].to_string(), s[pos + 1..].to_string()))
}

use etna::{
    commands::{
        self,
        experiment::visualize::{MetricType, VisualizationType},
    },
    error_context::Context,
    experiment::ExperimentMetadata,
    manager::Manager,
};

use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    filter, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer as _,
};

/// Call this early in `main`. Keep the returned `WorkerGuard` alive
/// (e.g., store in a global or a field) so file logs get flushed.
pub fn init_tracing() -> anyhow::Result<WorkerGuard> {
    // Base filter:
    // - If ETNA_LOG exists, use it.
    // - Else default to `info` and clamp some noisy modules.
    let mut base_filter = if env::var_os("ETNA_LOG").is_some() {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    } else {
        // You can add more directives here if you like.
        EnvFilter::new("info,marauders=error,ignore=error")
    };

    base_filter = base_filter.add_directive("marauders=error".parse()?);
    base_filter = base_filter.add_directive("ignore=error".parse()?);

    // -------- File layer (no ANSI) --------
    // Append-only file (no rotation). Swap `never` with `daily` if you want rotation.
    let file_appender = tracing_appender::rolling::never(".", "etna.log");
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true)
        .with_level(true)
        .without_time() // add a timer if you want timestamps
        // .with_timer(fmt::time::OffsetTime::local_rfc_3339().unwrap())
        ;

    // -------- Stdout layer(s) --------
    let stdout_is_colored_mode = env::var_os("ETNA_LOG").is_some();

    // Case 1: ETNA_LOG is set → one colored stdout layer, normal formatting
    let stdout_layer_colored = fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .with_target(false)
        .with_level(true)
        .with_file(true)
        .with_line_number(true)
        .without_time();

    // Case 2: ETNA_LOG is NOT set → two stdout layers:
    //  (a) INFO-only, plain message (println-like, no ANSI)
    let info_only_filter = filter::filter_fn(|meta| meta.level() == &Level::INFO);
    let stdout_info_plain = fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(false) // plain
        .with_target(false) // just the message
        .with_level(false)
        .without_time()
        .with_filter(info_only_filter);

    //  (b) WARN/ERROR with small colored headers so they’re visible
    let warn_error_filter =
        filter::filter_fn(|meta| matches!(*meta.level(), Level::WARN | Level::ERROR));
    let stdout_warn_err = fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .with_target(true)
        .with_level(true)
        .with_file(true)
        .with_line_number(true)
        .without_time()
        .with_filter(warn_error_filter);

    // Build subscriber
    let registry = tracing_subscriber::registry().with(base_filter);

    if stdout_is_colored_mode {
        registry
            .with(file_layer)
            .with(stdout_layer_colored)
            .try_init()?;
    } else {
        registry
            .with(file_layer)
            .with(stdout_info_plain)
            .with(stdout_warn_err)
            .try_init()?;
    }

    Ok(guard)
}

fn main() -> anyhow::Result<()> {
    let _guard = init_tracing()?;
    // Invoke the CLI
    run()
}

pub(crate) fn run() -> anyhow::Result<()> {
    let cli = Args::parse();

    // Handle commands that don't need the manager
    if let Command::Completions { shell } = &cli.command {
        let mut cmd = Args::command();
        clap_complete::generate(*shell, &mut cmd, "etna", &mut std::io::stdout());
        return Ok(());
    }

    if let Command::Setup { .. } = &cli.command {
        // Skip loading the manager for setup command
        return commands::config::setup::invoke(
            matches!(cli.command, Command::Setup { overwrite } if overwrite),
        );
    }

    // `workload site` only uses the bundled catalog + HTTP; it doesn't touch
    // any experiment state, so it runs fine without `etna setup`. CI in
    // particular benefits — the catalog-deploy workflow has no persisted
    // config on the runner.
    if let Command::Workload(WorkloadCommand::Site { out, catalog }) = cli.command {
        return commands::workload::site::invoke(out, catalog);
    }

    let mut mgr = Manager::load().context("All commands other than `etna setup` require a valid configuration, please make sure you ran `etna setup` first")?;

    let experiment = if let Some(experiment) = cli.command.experiment_name() {
        if let Some(experiment) = mgr.experiments.get(experiment) {
            Some(experiment.clone())
        } else {
            anyhow::bail!("experiment '{}' is not found, please run `etna experiment list` to see available experiments", experiment);
        }
    } else {
        // Check if the CWD is a subdirectory of an experiment
        if let Ok(experiment) = ExperimentMetadata::from_current_dir(&mgr) {
            Some(experiment)
        } else if cli.command.requires_experiment() {
            anyhow::bail!("no experiment name is provided, and the current dir is not an experiment directory, please run `etna experiment list` to see available experiments");
        } else {
            None
        }
    };

    if let Some(experiment) = &experiment {
        mgr.set_store_path(experiment.store.clone())?;
    }

    match cli.command {
        Command::Experiment(exp) => match exp {
            ExperimentCommand::New {
                name,
                path,
                overwrite,
            } => commands::experiment::new::invoke(mgr, name, path, overwrite),
            ExperimentCommand::Register { name, path } => {
                commands::experiment::register::invoke(mgr, name, path)
            }
            ExperimentCommand::Clone { url, path, reference } => {
                commands::experiment::clone::invoke(mgr, url, path, reference)
            }
            ExperimentCommand::Run { name: _, tests, short_circuit, parallel, params, store } => {
                let mut experiment = experiment.unwrap();
                if let Some(path) = store {
                    experiment.store = path;
                }
                commands::experiment::run::invoke(mgr, experiment, tests, short_circuit, parallel, params)
            },
            ExperimentCommand::Show {
                        name,
                    } => commands::experiment::show::invoke(mgr, name),
            ExperimentCommand::CreateTest { name: _, workload, test, trials, timeout, mode, mutation } => {
                commands::experiment::create_test::invoke(
                    mgr,
                    experiment.unwrap(),
                    workload,
                    test,
                    trials,
                    timeout,
                    mode,
                    mutation,
                )
            }
            ExperimentCommand::AmendTest { name: _, test, strategy, mutation, property } => {
                commands::experiment::amend_test::invoke(
                    mgr,
                    experiment.unwrap(),
                    test,
                    strategy,
                    mutation,
                    property,
                )
            }
            ExperimentCommand::Visualize { name: _, figure, tests, groupby, aggby, metric, buckets, max, visualization_type, hatched, store } => {
                let mut experiment = experiment.unwrap();
                if let Some(path) = store {
                    experiment.store = path;
                }
                commands::experiment::visualize::invoke(mgr, experiment, figure, tests, groupby, aggby, metric, buckets, max, visualization_type, hatched)
            },
            ExperimentCommand::VisualizeJson { input, output } => commands::experiment::visualize::draw_bucket_chart_from_json(&input, &output),
            ExperimentCommand::Report { name: _, output, publish, strip_counterexamples, json_output, store } => {
                let mut experiment = experiment.unwrap();
                if let Some(path) = store {
                    experiment.store = path;
                }
                commands::experiment::report::invoke(mgr, experiment, output, publish, strip_counterexamples, json_output)
            },
            ExperimentCommand::PublishPage { name: _, output, strip_counterexamples } => commands::experiment::publish_page::invoke(mgr, experiment.unwrap(), output, strip_counterexamples),
            ExperimentCommand::List {} => commands::experiment::list::invoke(mgr),
        },
        Command::Workload(wl) => match wl {
            WorkloadCommand::AddWorkload {
                experiment: _,
                spec,
                reference,
            } => commands::workload::add_workload::invoke(mgr, experiment.unwrap(), spec, reference),
            WorkloadCommand::RemoveWorkload {
                experiment: _,
                workload,
            } => commands::workload::remove_workload::invoke(experiment.unwrap(), workload)
                .context("Try running `etna workload remove` in an experiment directory, or explicitly specify the experiment name with `etna workload remove --experiment <NAME>`"),
            WorkloadCommand::ListWorkloads {
                experiment: _,
                kind,
            } => commands::workload::list_workloads::invoke(mgr, experiment.unwrap(), kind),
            WorkloadCommand::Update {} => commands::workload::update_index::invoke(),
            WorkloadCommand::Doc { dir } => commands::workload::doc::invoke(dir),
            WorkloadCommand::Check { dir } => commands::workload::check::invoke(dir),
            WorkloadCommand::Site { out, catalog } => commands::workload::site::invoke(out, catalog),
        },
        Command::Config(cl) => match cl {
            ConfigCommand::Show => commands::config::show::invoke(),
        },
        Command::Setup { .. } => unreachable!("Setup command is handled earlier"),
        Command::Analyze(_analyze_command) => todo!(),
        Command::Mutation(mutation_command) => match mutation_command {
            MutationCommand::List { path } => commands::mutation::list::invoke(path),
            MutationCommand::Set { variant, path, glob } => {
                commands::mutation::set::invoke(path, variant, glob)
            }
            MutationCommand::Reset { path } => commands::mutation::reset::invoke(path),
        },
        Command::Check { restore, remove } => commands::check::integrity::invoke(mgr, restore, remove),
        #[cfg(unix)]
        Command::Bash { path } => commands::bash::invoke(mgr, path),
        Command::Completions { .. } => unreachable!("Completions command is handled earlier"),
    }
    .context("Aborting run due to an error")
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum ExperimentCommand {
    #[clap(name = "new", about = "Create a new experiment")]
    New {
        /// Name of the new experiment
        name: String,
        /// An optional root path, if not provided, the current directory is used
        path: Option<PathBuf>,
        /// Overwrite the existing experiment
        #[clap(short = 'o', long)]
        overwrite: bool,
    },
    #[clap(
        name = "register",
        about = "Register an existing experiment in the tracking metadata"
    )]
    Register {
        /// Name of the experiment, if not provided, the experiment name is inferred from the experiment path
        name: Option<String>,
        /// An optional root path, if not provided, the current directory is used
        path: Option<PathBuf>,
    },
    #[clap(
        name = "clone",
        about = "Clone a remote experiment repo and register it"
    )]
    Clone {
        /// Git URL of the experiment repo (root must contain `etna.toml`)
        url: String,
        /// Parent directory to clone into. Defaults to the current directory.
        /// The experiment lands at `<path>/<name>/` where `<name>` comes from
        /// the repo's `etna.toml`.
        #[clap(long)]
        path: Option<PathBuf>,
        /// Optional branch/tag/commit to check out (passed as `--branch`).
        #[clap(long = "ref")]
        reference: Option<String>,
    },
    #[clap(name = "run", about = "Run an experiment")]
    Run {
        /// Name of the experiment to run
        /// [default: current directory]
        #[clap(short, long)]
        name: Option<String>,
        /// A list of tests to run given as file name stems from the `tests` directory
        #[clap(long)]
        tests: Vec<String>,
        /// Short circuit the trials if any test fails
        #[clap(short = 's', long, default_value = "false")]
        short_circuit: bool,
        /// Run the tests in parallel
        /// [default: false]
        /// Note: Parallel execution requires the run to be pure, if it's effectful, it may lead to unexpected results.
        #[clap(short = 'p', long, default_value = "false")]
        parallel: bool,
        /// Additional parameters in key=value format
        /// These override parameters defined in test JSON files
        #[clap(long, value_parser = parse_key_value)]
        params: Vec<(String, String)>,
        /// Override the path to the metric store (default: <experiment>/store.jsonl)
        #[clap(long)]
        store: Option<PathBuf>,
    },
    #[clap(name = "show", about = "Show the details of an experiment")]
    Show {
        /// Name
        #[clap(long)]
        name: String,
    },
    #[clap(
        name = "create-test",
        about = "Create a new test file with default values"
    )]
    CreateTest {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long)]
        name: Option<String>,
        /// Workload name (must already be added to the experiment)
        #[clap(long)]
        workload: String,
        /// Test file name (without .json extension)
        /// [default: <workload>]
        #[clap(long)]
        test: Option<String>,
        /// Number of trials
        #[clap(long, default_value = "10")]
        trials: usize,
        /// Timeout in seconds
        #[clap(long, default_value = "60")]
        timeout: f64,
        /// Experiment mode: solve | sample | test | shrink | cross
        #[clap(long, default_value = "solve")]
        mode: String,
        /// Mutation variant(s)
        #[clap(long)]
        mutation: Vec<String>,
    },
    #[clap(
        name = "amend-test",
        about = "Amend an existing test file by adding/duplicating tasks with a strategy"
    )]
    AmendTest {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long)]
        name: Option<String>,
        /// Test name from tests directory (with or without .json)
        #[clap(long)]
        test: String,
        /// Strategy name to apply
        #[clap(long)]
        strategy: String,
        /// Optional mutation filter(s)
        #[clap(long)]
        mutation: Vec<String>,
        /// Optional property filter(s)
        #[clap(long)]
        property: Vec<String>,
    },
    #[clap(name = "visualize", about = "Visualize the results of the experiment")]
    Visualize {
        /// Name of the experiment to visualize
        #[clap(long)]
        name: Option<String>,
        /// Figure name
        #[clap(long)]
        figure: String,
        /// Tests to visualize the results of
        #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
        tests: Vec<String>,
        /// Group by fields
        #[clap(short, long, default_values_t = vec!["workload".to_string(), "strategy".to_string(), "mode".to_string()])]
        groupby: Vec<String>,
        /// Aggregate by fields
        #[clap(short, long, default_values_t = vec!["workload".to_string(), "strategy".to_string(), "property".to_string(), "mutations".to_string(), "mode".to_string()])]
        aggby: Vec<String>,
        /// Metric to visualize
        /// [default: "time"]
        /// [possible_values(time, memory, size, coverage)]
        #[clap(short, long, default_value_t = MetricType::Time)]
        metric: MetricType,
        /// Buckets to use for the visualization
        #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ', default_values_t = vec![0.1, 1.0, 10.0, 60.0])]
        buckets: Vec<f64>,
        /// Maximum value for the bar chart
        /// [default: None]
        #[clap(long)]
        max: Option<f64>,
        /// Type of visualization
        /// [default: "bucket"]
        /// [possible_values(line, bar, bucket)]
        #[clap(short, long, default_value = "VisualizationType::Bucket")]
        visualization_type: VisualizationType,
        /// Indices of groups to render with hatched pattern (0-indexed, comma-separated)
        /// e.g., --hatched 1,3 for every other group starting from index 1
        #[clap(long, value_parser, num_args = 0.., value_delimiter = ',')]
        hatched: Vec<usize>,
        /// Override the path to the metric store (default: <experiment>/store.jsonl)
        #[clap(long)]
        store: Option<PathBuf>,
    },
    #[clap(
        name = "visualize-json",
        about = "Render bucket chart from a pre-computed JSON file"
    )]
    VisualizeJson {
        /// Input JSON file path
        #[clap(short, long)]
        input: PathBuf,
        /// Output PNG file path
        #[clap(short, long)]
        output: PathBuf,
    },
    #[clap(
        name = "report",
        about = "Generate an interactive HTML report for the experiment"
    )]
    Report {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long)]
        name: Option<String>,
        /// Output file path
        /// [default: <experiment_path>/report.html]
        #[clap(short, long)]
        output: Option<PathBuf>,
        /// Publish the report as a public GitHub Gist (requires `gh` CLI)
        /// and print a viewable URL via gisthost.github.io
        #[clap(long, default_value = "false")]
        publish: bool,
        /// Drop per-trial `counterexample` fields from the embedded metrics
        /// payload. Useful when counterexamples blow the Gist size limit,
        /// at the cost of losing them from the interactive report.
        #[clap(long, default_value = "false")]
        strip_counterexamples: bool,
        /// Also dump the report's metrics payload as a standalone JSON file
        /// (the same data the HTML embeds, gzip-decoded and indented). Useful
        /// for serving a static `/json` endpoint alongside the HTML report.
        #[clap(long)]
        json_output: Option<PathBuf>,
        /// Override the path to the metric store (default: <experiment>/store.jsonl)
        #[clap(long)]
        store: Option<PathBuf>,
    },
    #[clap(
        name = "publish-page",
        about = "Bundle experiment results into a Cloudflare-Pages-ready directory"
    )]
    PublishPage {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long)]
        name: Option<String>,
        /// Output directory (created if missing). Receives index.html,
        /// report.html, report.json, store.jsonl, and etna.toml.
        #[clap(short, long)]
        output: PathBuf,
        /// Drop per-trial `counterexample` fields from the embedded metrics
        /// payload. Same effect as on `report`.
        #[clap(long, default_value = "false")]
        strip_counterexamples: bool,
    },
    #[clap(name = "list", about = "List all experiments")]
    List {},
}
#[derive(Debug, Subcommand)]
enum WorkloadCommand {
    #[clap(
        name = "add",
        about = "Add a workload to the experiment by catalog name or git URL"
    )]
    AddWorkload {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long, default_value = None)]
        experiment: Option<String>,
        /// Catalog name (e.g. `bst-haskell`) or git URL of the workload repo
        /// (must contain `etna.toml` + `steps.json` at its root).
        /// Run `etna workload list --kind available` to see catalog names.
        spec: String,
        /// Optional branch/tag/ref to clone. Overrides the catalog's
        /// `default_ref` when both are set.
        #[clap(long = "ref")]
        reference: Option<String>,
    },
    #[clap(name = "remove", about = "Remove a workload from the experiment")]
    RemoveWorkload {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long, default_value = None)]
        experiment: Option<String>,
        /// Name of the workload to remove (as declared in its etna.toml)
        workload: String,
    },
    #[clap(name = "list", about = "List all workloads")]
    ListWorkloads {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long, default_value = None)]
        experiment: Option<String>,
        /// Available or experiment workloads
        /// [possible_values(available, experiment)]
        #[clap(short, long, default_value = "experiment")]
        kind: String,
    },
    #[clap(
        name = "update",
        about = "Refresh the cached workload catalog from its canonical URL"
    )]
    Update {},
    #[clap(
        name = "doc",
        about = "Regenerate BUGS.md and TASKS.md from a workload's etna.toml"
    )]
    Doc {
        /// Path to the workload directory (contains `etna.toml`).
        /// Defaults to the current directory.
        #[clap(default_value = ".")]
        dir: PathBuf,
    },
    #[clap(
        name = "check",
        about = "Verify workload consistency: manifest parses, mutations match disk, witnesses/properties exist, patch files apply, docs are up to date"
    )]
    Check {
        /// Path to the workload directory (contains `etna.toml`).
        /// Defaults to the current directory.
        #[clap(default_value = ".")]
        dir: PathBuf,
    },
    #[clap(
        name = "site",
        about = "Publish the workload catalog as a static site (fetches etna.toml + patches per entry)"
    )]
    Site {
        /// Output directory. Will be populated with `data/catalog.json`
        /// and `data/workloads/<name>.json`. Drop the webview-ui `site.html`
        /// bundle alongside it to deploy.
        #[clap(short, long, default_value = "site")]
        out: PathBuf,
        /// Optional path to a catalog JSON file. Defaults to the cached
        /// workload index (bundled snapshot on first run).
        #[clap(long)]
        catalog: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
enum StoreCommand {
    #[clap(name = "write", about = "Write a metric to the store")]
    Write {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long, default_value = None)]
        experiment: Option<String>,
        /// Experiment ID
        experiment_id: String,
        /// Metric as a json string
        metric: String,
    },
    #[command(name = "query", about = "Query the store")]
    Query {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long, default_value = None)]
        experiment: Option<String>,
        /// Query string
        filter: String,
    },
    #[command(
        name = "remove",
        about = "Remove metrics from the store based on a filter"
    )]
    Remove {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long, default_value = None)]
        experiment: Option<String>,
        /// Filter to apply to the metrics
        filter: String,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    #[command(name = "show", about = "Show the current configuration")]
    Show,
}

#[derive(Debug, Subcommand)]
enum AnalyzeCommand {
    #[clap(name = "bucket", about = "Create bucket charts for the experiment")]
    BucketGen {
        /// Name of the experiment to run
        /// [default: current directory]
        #[clap(short, long)]
        name: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum MutationCommand {
    #[clap(name = "list", about = "List all mutations in a directory")]
    List {
        /// Path to the directory to scan for mutations
        #[clap(short, long, default_value = ".")]
        path: PathBuf,
    },
    #[clap(name = "set", about = "Activate a mutation variant")]
    Set {
        /// The variant name to activate
        variant: String,
        /// Path to the directory containing mutation files
        #[clap(short, long, default_value = ".")]
        path: PathBuf,
        /// Optional glob pattern to filter files
        #[clap(short, long)]
        glob: Option<String>,
    },
    #[clap(name = "reset", about = "Reset all mutations to default")]
    Reset {
        /// Path to the directory to reset mutations in
        #[clap(short, long, default_value = ".")]
        path: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(subcommand, name = "experiment", about = "Manage experiments")]
    Experiment(ExperimentCommand),
    #[command(subcommand, name = "workload", about = "Manage workloads")]
    Workload(WorkloadCommand),
    #[command(subcommand, name = "config", about = "Manage etna-cli configuration")]
    Config(ConfigCommand),
    #[command(name = "setup", about = "Setup etna-cli")]
    Setup {
        /// Overwrite the existing configuration
        #[clap(short, long, default_value = "false")]
        overwrite: bool,
    },
    #[command(name = "check", about = "Run checks on etna")]
    Check {
        /// Restore the store from the backup
        #[clap(long, default_value = "false")]
        restore: bool,
        /// Remove the store
        #[clap(long, default_value = "false")]
        remove: bool,
    },
    #[command(
        subcommand,
        name = "analyze",
        about = "Run analysis on results of the experiments"
    )]
    Analyze(AnalyzeCommand),
    #[command(subcommand, name = "mutation", about = "Manage mutations")]
    Mutation(MutationCommand),
    #[cfg(unix)]
    #[command(
        name = "bash",
        about = "Generate a bash script from a workload configuration"
    )]
    Bash {
        /// Path of the `config.toml`
        /// [default: current directory]
        #[clap(short, long, default_value = None)]
        path: Option<PathBuf>,
    },
    #[command(name = "completions", about = "Generate shell completions")]
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },
}

impl Command {
    pub fn experiment_name(&self) -> Option<&String> {
        match self {
            Command::Experiment(exp) => match exp {
                ExperimentCommand::New { .. } => None,
                ExperimentCommand::Register { .. } => None,
                ExperimentCommand::Clone { .. } => None,
                ExperimentCommand::Run { name, .. } => name.as_ref(),
                ExperimentCommand::Show { name, .. } => Some(name),
                ExperimentCommand::CreateTest { name, .. } => name.as_ref(),
                ExperimentCommand::AmendTest { name, .. } => name.as_ref(),
                ExperimentCommand::Visualize { name, .. } => name.as_ref(),
                ExperimentCommand::VisualizeJson { .. } => None,
                ExperimentCommand::Report { name, .. } => name.as_ref(),
                ExperimentCommand::PublishPage { name, .. } => name.as_ref(),
                ExperimentCommand::List { .. } => None,
            },
            Command::Workload(wl) => match wl {
                WorkloadCommand::AddWorkload { experiment, .. } => experiment.as_ref(),
                WorkloadCommand::RemoveWorkload { experiment, .. } => experiment.as_ref(),
                WorkloadCommand::ListWorkloads { experiment, .. } => experiment.as_ref(),
                WorkloadCommand::Update {} => None,
                WorkloadCommand::Doc { .. } => None,
                WorkloadCommand::Check { .. } => None,
                WorkloadCommand::Site { .. } => None,
            },
            _ => None,
        }
    }

    pub fn requires_experiment(&self) -> bool {
        match self {
            Command::Experiment(exp) => match exp {
                ExperimentCommand::New { .. } => false,
                ExperimentCommand::Register { .. } => false,
                ExperimentCommand::Clone { .. } => false,
                ExperimentCommand::Run { .. } => true,
                ExperimentCommand::Show { .. } => true,
                ExperimentCommand::CreateTest { .. } => true,
                ExperimentCommand::AmendTest { .. } => true,
                ExperimentCommand::Visualize { .. } => true,
                ExperimentCommand::VisualizeJson { .. } => false,
                ExperimentCommand::Report { .. } => true,
                ExperimentCommand::PublishPage { .. } => true,
                ExperimentCommand::List { .. } => false,
            },
            Command::Workload(wl) => match wl {
                WorkloadCommand::AddWorkload { .. } => true,
                WorkloadCommand::RemoveWorkload { .. } => true,
                WorkloadCommand::ListWorkloads { .. } => true,
                WorkloadCommand::Update {} => false,
                WorkloadCommand::Doc { .. } => false,
                WorkloadCommand::Check { .. } => false,
                WorkloadCommand::Site { .. } => false,
            },
            _ => false,
        }
    }
}
