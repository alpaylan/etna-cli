use crate::{config::EtnaConfig, store::Store};

pub(crate) fn invoke(hash_or_name: String, is_name: bool, show_all: bool) -> anyhow::Result<()> {
    let etna_config = EtnaConfig::get_etna_config()?;

    let store = Store::load(&etna_config.store_path())?;

    match (is_name, show_all) {
        (true, true) => {
            let experiments = store.get_all_experiments_by_name(&hash_or_name);
            for experiment in experiments {
                println!("{:#?}", experiment);
            }
        }
        (true, false) => {
            println!("{:#?}", store.get_experiment_by_name(&hash_or_name)?);
        }
        (false, _) => {
            println!("{:#?}", store.get_experiment_by_id(&hash_or_name)?);
        }
    };

    Ok(())
}
