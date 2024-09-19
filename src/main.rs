use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod commands;
mod config;
mod experiment;
mod git_driver;
mod python_driver;
mod snapshot;
mod store;
mod workload;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum ExperimentCommand {
    #[clap(name = "new", about = "Create a new experiment")]
    NewExperiment {
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
    RunExperiment {
        /// Name of the experiment to run
        /// [default: current directory]
        #[clap(short, long)]
        name: Option<String>,
    },
}
#[derive(Debug, Subcommand)]
enum WorkloadCommand {
    #[clap(name = "add", about = "Add a workload to the experiment")]
    AddWorkload {
        /// Name of the experiment
        /// [default: current directory]
        #[clap(short, long, default_value = None)]
        name: Option<String>,
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
        name: Option<String>,
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
        name: Option<String>,
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
        /// Branch to clone the etna repository
        #[clap(short, long, default_value = "main")]
        branch: String,
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
}

fn main() -> anyhow::Result<()> {
    let cli = Args::parse();
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    match cli.command {
        Command::Experiment(exp) => match exp {
            ExperimentCommand::NewExperiment {
                name,
                path,
                overwrite,
                description,
            } => commands::experiment::new_experiment::invoke(name, path, overwrite, description),
            ExperimentCommand::RunExperiment { name } => {
                commands::experiment::run_experiment::invoke(name)
            }
        },
        Command::Workload(wl) => match wl {
            WorkloadCommand::AddWorkload {
                name,
                language,
                workload,
            } => commands::workload::add_workload::invoke(name, language, workload),
            WorkloadCommand::RemoveWorkload {
                name,
                language,
                workload,
            } => commands::workload::remove_workload::invoke(name, language, workload),
            WorkloadCommand::ListWorkloads {
                name,
                language,
                kind,
            } => commands::workload::list_workloads::invoke(name, language, kind),
        },
        Command::Config(cl) => match cl {
            ConfigCommand::ChangeBranch { branch } => {
                commands::config::change_branch::invoke(branch)
            }
        },
        Command::Setup { overwrite, branch } => commands::config::setup::invoke(overwrite, branch),
    }
}
