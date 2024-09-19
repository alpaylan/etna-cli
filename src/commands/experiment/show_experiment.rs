use crate::{config::EtnaConfig, store::Store};

pub(crate) fn invoke(hash: String) -> anyhow::Result<()> {
    let etna_config = EtnaConfig::get_etna_config()?;

    let store = Store::load(&etna_config.store_path())?;

    let experiment = store.get_experiment_by_id(&hash)?;

    println!("{:#?}", experiment);

    Ok(())
}