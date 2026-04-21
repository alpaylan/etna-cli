use crate::{manager::Manager, service};

pub fn invoke(mgr: Manager, name: String) -> anyhow::Result<()> {
    let experiment = service::experiment::get_experiment(&mgr, &name)?;

    println!("Experiment: {}", experiment.name);
    println!("Path: {}", experiment.path.display());
    println!("Workloads:");
    for wl in experiment.workloads {
        println!("- {}", wl.name);
    }

    Ok(())
}
