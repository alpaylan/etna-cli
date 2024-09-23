
use anyhow::Context;
use lib::{handle_jq_query, handle_specialized_query};

use crate::{
    cli::QueryOption, config::EtnaConfig, store::Store
};

mod lib;

pub(crate) fn invoke(query_option: QueryOption) -> anyhow::Result<()> {
    let etna_config = EtnaConfig::get_etna_config()?;
    let store = Store::load(&etna_config.store_path())?;

    let use_jq = std::env::var("ETNA_USE_JQ")
        .unwrap_or("false".to_string())
        .parse::<bool>()?;

    match query_option {
        QueryOption::Jq { .. }
        | QueryOption::MetricsByFields { .. }
        | QueryOption::SnapshotsByFields { .. } => {
            handle_jq_query(store, query_option).context("Failed to handle jq query")
        }
        _ => {
            if use_jq {
                handle_jq_query(store, query_option).context("Failed to handle jq query")
            } else {
                handle_specialized_query(store, query_option)
                    .context("Failed to handle special query")
            }
        }
    }
}