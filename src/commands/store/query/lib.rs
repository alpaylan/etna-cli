use std::rc::Rc;

use crate::{
    cli::QueryOption,
    store::{self, ExperimentQuery, MetricQuery, Queriable, SpecializedQuery, Store},
};

use anyhow::Context;
use jaq_interpret::{Ctx, Error, FilterT, ParseCtx, RcIter, Val};

use serde_json::Value;

fn jaq_val_to_serde_value(v: Val) -> Value {
    match v {
        Val::Null => Value::Null,
        Val::Bool(b) => Value::Bool(b),
        Val::Num(n) => serde_json::from_str(&n).unwrap(),
        Val::Str(s) => Value::String(s.to_string()),
        Val::Arr(a) => {
            let a = Rc::try_unwrap(a).unwrap();
            Value::Array(a.into_iter().map(jaq_val_to_serde_value).collect())
        }
        Val::Obj(o) => {
            let o = Rc::try_unwrap(o).unwrap();
            Value::Object(
                o.into_iter()
                    .map(|(k, v)| (k.to_string(), jaq_val_to_serde_value(v)))
                    .collect(),
            )
        }
        Val::Int(i) => Value::Number(serde_json::Number::from(i)),
        Val::Float(f) => Value::Number(serde_json::Number::from_f64(f).unwrap()),
    }
}

fn jaq_error_to_anyhow_error(e: Error) -> anyhow::Error {
    match e {
        Error::Val(v) => anyhow::Error::msg(format!("Value '{:?}' is not a valid jaq value", v)),
        Error::Type(v, t) => {
            anyhow::Error::msg(format!("Value '{:?}' is not of type '{:?}'", v, t))
        }
        Error::MathOp(a, op, b) => anyhow::Error::msg(format!(
            "Math operation '{:?}' is not valid between '{:?}' and '{:?}'",
            op, a, b
        )),
        Error::Index(val, index) => anyhow::Error::msg(format!(
            "Value '{:?}' cannot be indexed with '{:?}'",
            val, index
        )),
        Error::IndexOutOfBounds(index) => {
            anyhow::Error::msg(format!("Index '{:?}' is out of bounds", index))
        }
        Error::PathExp => anyhow::Error::msg("Path expression is not valid"),
        Error::TailCall(tail_call) => {
            anyhow::Error::msg(format!("Tail call '{:?}' is not valid", tail_call))
        }
        _ => anyhow::Error::msg("Unknown error"),
    }
}

fn jaq_handler(input: Value, program: &str) -> anyhow::Result<Value> {
    let mut defs = ParseCtx::new(Vec::new());
    defs.insert_natives(jaq_core::core());
    defs.insert_defs(jaq_std::std());

    let parser = jaq_parse::defs();

    // parse the include file
    let (f, errs) = jaq_parse::parse(include_str!("lib.jq"), parser);
    anyhow::ensure!(
        errs.is_empty(),
        format!("Failed to parse lib.jq {:?} with errors: {:?}", f, errs)
    );

    if let Some(f) = f {
        defs.insert_defs(f);
    } else {
        anyhow::bail!("Failed to parse lib.jq '{:?}' with errors: {:?}", f, errs);
    }

    // parse the filter
    let (f, errs) = jaq_parse::parse(program, jaq_parse::main());
    anyhow::ensure!(
        errs.is_empty(),
        format!(
            "Failed to parse the jq program {:?} with errors: {:?}",
            program, errs
        )
    );

    anyhow::ensure!(
        f.is_some(),
        format!(
            "Failed to parse the jq program {:?} with errors: {:?}",
            f, errs
        )
    );

    // compile the filter in the context of the given definitions
    let f = defs.compile(f.unwrap());
    anyhow::ensure!(
        defs.errs.is_empty(),
        format!(
            "Failed to compile the jq program with errors: {:?}",
            defs.errs
                .iter()
                .map(|e| format!("({}, {:?})", e.0, e.1))
                .collect::<Vec<String>>()
        )
    );

    let inputs = RcIter::new(core::iter::empty());

    // iterator over the output values
    let out = f.run((Ctx::new([], &inputs), Val::from(input)));

    // collect the output values into a vector
    let mut res = Vec::new();
    for v in out {
        let v = v.unwrap_or_else(|e| panic!("{}", jaq_error_to_anyhow_error(e)));
        res.push(v);
    }

    let res = res.into_iter().map(jaq_val_to_serde_value).collect();
    Ok(res)
}

pub(crate) fn handle_jq_query(store: Store, query_option: QueryOption) -> anyhow::Result<()> {
    let query_string = match query_option {
        QueryOption::Jq { query_string } => query_string,
        QueryOption::ExperimentById { experiment_id } => {
            format!(r#"experiment_by_id("{}")"#, experiment_id)
        }
        QueryOption::ExperimentByName { experiment_name } => {
            format!(r#"last_experiment_by_name("{}")"#, experiment_name)
        }
        QueryOption::AllExperimentsByName { experiment_name } => {
            format!(r#"experiments_by_name("{}")"#, experiment_name)
        }
        QueryOption::MetricsByExperimentId { experiment_id } => {
            format!(r#"metrics_by_experiment_id("{}")"#, experiment_id)
        }
        QueryOption::MetricsByFields { fields_json_string } => {
            let fields_json: serde_json::Value = serde_json::from_str(&fields_json_string)
                .context("Failed to parse the fields json string")?;

            format!(r#"metrics_by_json_string({:?})"#, fields_json.to_string())
        }
        QueryOption::SnapshotsByFields { fields_json_string } => {
            let fields_json: serde_json::Value = serde_json::from_str(&fields_json_string)
                .context("Failed to parse the fields json string")?;

            format!(r#"snapshots_by_json_string({:?})"#, fields_json.to_string())
        }
        QueryOption::SnapshotsByName { snapshot_name } => {
            format!(r#"snapshots_by_name("{}")"#, snapshot_name)
        }
        QueryOption::SnapshotByHash { snapshot_hash } => {
            format!(r#"snapshot_by_hash("{}")"#, snapshot_hash)
        }
    };

    let result = jaq_handler(serde_json::json!(store), &query_string)
        .context(format!("jq query '{query_string}' has failed"))?;

    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}

pub(crate) fn handle_specialized_query(
    store: Store,
    query_option: QueryOption,
) -> anyhow::Result<()> {
    let query = match query_option {
        QueryOption::Jq { .. }
        | QueryOption::MetricsByFields { .. }
        | QueryOption::SnapshotsByFields { .. } => {
            anyhow::bail!("Unreachable, should have been handled by handle_jq_query")
        }
        QueryOption::ExperimentById { experiment_id } => {
            SpecializedQuery::Experiment(ExperimentQuery::Id(experiment_id))
        }
        QueryOption::ExperimentByName { experiment_name } => {
            SpecializedQuery::Experiment(ExperimentQuery::NameLast(experiment_name))
        }
        QueryOption::AllExperimentsByName { experiment_name } => {
            SpecializedQuery::Experiment(ExperimentQuery::NameAll(experiment_name))
        }
        QueryOption::MetricsByExperimentId { experiment_id } => {
            SpecializedQuery::Metric(MetricQuery::ByExperimentId(experiment_id))
        }
        QueryOption::SnapshotsByName { snapshot_name } => {
            SpecializedQuery::Snapshot(store::SnapshotQuery::ByName(snapshot_name))
        }
        QueryOption::SnapshotByHash { snapshot_hash } => {
            SpecializedQuery::Snapshot(store::SnapshotQuery::ByHash(snapshot_hash))
        }
    };

    let results = query
        .query(&store)
        .context("Querying the store has failed")?;

    for result in results {
        println!("{}", result);
    }

    Ok(())
}
