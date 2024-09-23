use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::commands;

pub(crate) fn run() -> anyhow::Result<()> {
    let cli = Args::parse();

    match cli.command {
        Command::Experiment(exp) => match exp {
            ExperimentCommand::New {
                name,
                path,
                overwrite,
                description,
            } => commands::experiment::new_experiment::invoke(name, path, overwrite, description),
            ExperimentCommand::Run { name } => {
                commands::experiment::run_experiment::invoke(name)
            }
            ExperimentCommand::Show {
                hash_or_name,
                is_name,
                show_all,
            } => commands::experiment::show_experiment::invoke(hash_or_name, is_name, show_all),
        },
        Command::Workload(wl) => match wl {
            WorkloadCommand::AddWorkload {
                experiment,
                language,
                workload,
            } => commands::workload::add_workload::invoke(experiment, language, workload),
            WorkloadCommand::RemoveWorkload {
                experiment,
                language,
                workload,
            } => commands::workload::remove_workload::invoke(experiment, language, workload),
            WorkloadCommand::ListWorkloads {
                experiment,
                language,
                kind,
            } => commands::workload::list_workloads::invoke(experiment, language, kind),
        },
        Command::Config(cl) => match cl {
            ConfigCommand::ChangeBranch { branch } => {
                commands::config::change_branch::invoke(branch)
            }
            ConfigCommand::Show => commands::config::show::invoke(),
        },
        Command::Setup {
            overwrite,
            branch,
            repo_path,
        } => commands::config::setup::invoke(overwrite, branch, repo_path),
        Command::Store(store_command) => match store_command {
            StoreCommand::Write {
                experiment_id,
                metric,
            } => commands::store::write::invoke(experiment_id, metric),
            StoreCommand::Query(query_option) => commands::store::query::invoke(query_option),
        },
    }
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
        path: Option<PathBuf>,
        /// Overwrite the existing experiment
        #[clap(short = 'o', long)]
        overwrite: bool,
        /// Description of the experiment
        /// [default: A description of the experiment]
        #[clap(short = 'd', long)]
        description: Option<String>,
    },
    #[clap(name = "run", about = "Run an experiment")]
    Run {
        /// Name of the experiment to run
        /// [default: current directory]
        #[clap(short, long)]
        name: Option<String>,
    },
    #[clap(name = "show", about = "Show the details of an experiment")]
    Show {
        /// Hash or name of the experiment
        hash_or_name: String,
        /// Is the provided string a hash or a name
        #[clap(short = 'n', long, default_value = "false")]
        is_name: bool,
        /// Show all the experiments
        #[clap(short = 'a', long, default_value = "false")]
        show_all: bool,
    },
}
#[derive(Debug, Subcommand)]
enum WorkloadCommand {
    #[clap(name = "add", about = "Add a workload to the experiment")]
    AddWorkload {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long, default_value = None)]
        experiment: Option<String>,
        /// Language of the workload
        /// [default: coq]
        /// [possible_values(coq, haskell, racket)]
        language: String,
        /// Workload to be added
        /// [default: bst]
        /// [possible_values(bst, rbt, stlc, ifc)]
        workload: String,
    },
    #[clap(name = "remove", about = "Remove a workload from the experiment")]
    RemoveWorkload {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long, default_value = None)]
        experiment: Option<String>,
        /// Language of the workload
        /// [possible_values(coq, haskell, racket)]
        language: String,
        /// Workload to be removed
        /// [possible_values(bst, rbt, stlc, ifc)]
        workload: String,
    },
    #[clap(name = "list", about = "List all workloads")]
    ListWorkloads {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long, default_value = None)]
        experiment: Option<String>,
        /// Language of the workload
        /// [possible_values(coq, haskell, racket)]
        /// [default: all]
        #[clap(short, long, default_value = "all")]
        language: String,
        /// Available or experiment workloads
        /// [possible_values(available, experiment)]
        /// [default: experiment]
        #[clap(short, long, default_value = "experiment")]
        kind: String,
    },
}

#[derive(Debug, Subcommand)]
enum StoreCommand {
    #[clap(name = "write", about = "Write a metric to the store")]
    Write {
        /// Experiment ID
        experiment_id: String,
        /// Metric as a json string
        metric: String,
    },
    #[command(subcommand, name = "query", about = "Query the store")]
    Query(QueryOption),
}

#[derive(Debug, Subcommand)]
pub(crate) enum QueryOption {
    #[clap(name = "--jq", about = "JQ Query")]
    Jq {
        /// Query string
        query_string: String,
    },
    #[clap(name = "--experiment-by-id", about = "Get an experiment by id")]
    ExperimentById {
        /// Experiment ID
        experiment_id: String,
    },
    #[clap(name = "--experiment-by-name", about = "Get an experiment by name")]
    ExperimentByName {
        /// Experiment Name
        experiment_name: String,
    },
    #[clap(
        name = "--all-experiments-by-name",
        about = "Get all experiment for a given name"
    )]
    AllExperimentsByName {
        /// Experiment Name
        experiment_name: String,
    },
    #[clap(
        name = "--metrics-by-experiment-id",
        about = "Get all metrics for a given experiment id"
    )]
    MetricsByExperimentId {
        /// Experiment ID
        experiment_id: String,
    },
    #[clap(
        name = "--metrics-by-fields",
        about = "Get all metrics that match the given fields"
    )]
    MetricsByFields {
        /// Fields to match
        fields_json_string: String,
    },
    #[clap(
        name = "--snapshots-by-fields",
        about = "Get all snapshots that match the given fields"
    )]
    SnapshotsByFields {
        /// Fields to match
        fields_json_string: String,
    },
    #[clap(
        name = "--snapshots-by-name",
        about = "Get all snapshots for a given name"
    )]
    SnapshotsByName {
        /// Snapshot Name
        snapshot_name: String,
    },
    #[clap(
        name = "--snapshot-by-hash",
        about = "Get the snapshot for a given hash"
    )]
    SnapshotByHash {
        /// Snapshot Hash
        snapshot_hash: String,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    #[command(
        name = "change-branch",
        about = "Change the branch of the etna repository"
    )]
    ChangeBranch {
        /// Branch to clone the etna repository
        /// [default: main]
        #[clap(short, long, default_value = "main")]
        branch: String,
    },
    #[command(name = "show", about = "Show the current configuration")]
    Show,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(subcommand, name = "experiment", about = "Manage experiments")]
    Experiment(ExperimentCommand),
    #[command(subcommand, name = "workload", about = "Manage workloads")]
    Workload(WorkloadCommand),
    #[command(subcommand, name = "store", about = "Manage the etna store")]
    Store(StoreCommand),
    #[command(subcommand, name = "config", about = "Manage etna-cli configuration")]
    Config(ConfigCommand),
    #[command(name = "setup", about = "Setup etna-cli")]
    Setup {
        /// Overwrite the existing configuration
        #[clap(short, long, default_value = "false")]
        overwrite: bool,
        /// Branch to clone the etna repository
        #[clap(short, long, default_value = "main")]
        branch: String,
        /// Repository path, if already cloned
        #[clap(long, default_value = None)]
        repo_path: Option<String>,
    },
}
