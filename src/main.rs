
mod commands;
mod config;
mod experiment;
mod git_driver;
mod python_driver;
mod snapshot;
mod store;
mod workload;
mod cli;

fn main() -> anyhow::Result<()> {
    
    // Initialize the logger
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .init();

    // Invoke the CLI
    cli::run()   
}
