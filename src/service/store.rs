use jaq_interpret::{Ctx, FilterT as _, RcIter, Val};

use crate::{
    commands::store::lib::jaq_compile,
    error_context::Context,
    store::{Metric, Store},
};

use super::types::{QueryResult, RemoveMetricsOptions, ServiceResult, WriteMetricRequest};

/// Write a metric to the store
pub fn write_metric(store: &mut Store, request: WriteMetricRequest) -> ServiceResult<usize> {
    let WriteMetricRequest { hash, data } = request;

    tracing::debug!(
        "Adding metric for experiment with hash '{}': {}",
        hash,
        serde_json::to_string(&data).unwrap_or_else(|_| "Invalid JSON".to_string())
    );

    let count = store.push(Metric { hash, data })?;

    Ok(count)
}

/// Query metrics from the store using a JQ filter
pub fn query_metrics(store: &Store, filter: &str) -> ServiceResult<QueryResult> {
    // Load metrics into a JSON array and apply the filter
    let metrics_json = serde_json::json!(store.metrics);

    let result =
        jaq_handler(metrics_json, filter).context(format!("JQ query '{}' failed", filter))?;

    // Convert the result to a list of metrics
    let metrics = match result {
        serde_json::Value::Array(arr) => arr,
        other => vec![other],
    };

    Ok(QueryResult { metrics })
}

/// Remove metrics from the store that match the filter
pub fn remove_metrics(store: &mut Store, options: RemoveMetricsOptions) -> ServiceResult<usize> {
    let RemoveMetricsOptions { filter } = options;

    let compiled_filter = jaq_compile(&filter).context("Failed to compile jq query")?;

    let initial_count = store.metrics.len();

    store.retain(|metric| {
        let inputs = RcIter::new(core::iter::empty());
        let mut out = compiled_filter.run((
            Ctx::new([], &inputs),
            Val::from(serde_json::Value::Object(metric.data.clone())),
        ));
        let result = out.next();
        let Some(Ok(Val::Bool(true))) = result else {
            return true;
        };
        false
    });

    let removed_count = initial_count - store.metrics.len();

    Ok(removed_count)
}

/// Get all metrics from the store
pub fn get_all_metrics(store: &Store) -> ServiceResult<Vec<serde_json::Value>> {
    let metrics: Vec<serde_json::Value> = store
        .metrics
        .iter()
        .map(|m| {
            let mut obj = m.data.clone();
            obj.insert(
                "hash".to_string(),
                serde_json::Value::String(m.hash.clone()),
            );
            serde_json::Value::Object(obj)
        })
        .collect();

    Ok(metrics)
}

/// Load metrics from disk into the store
pub fn load_metrics(store: &mut Store) -> ServiceResult<()> {
    store.load_metrics()?;
    Ok(())
}

// Helper function for JQ handling
fn jaq_val_to_serde_value(v: Val) -> serde_json::Value {
    use std::rc::Rc;
    match v {
        Val::Null => serde_json::Value::Null,
        Val::Bool(b) => serde_json::Value::Bool(b),
        Val::Num(n) => serde_json::from_str(&n).unwrap(),
        Val::Str(s) => serde_json::Value::String(s.to_string()),
        Val::Arr(a) => {
            let a = Rc::try_unwrap(a).unwrap_or_else(|rc| (*rc).clone());
            serde_json::Value::Array(a.into_iter().map(jaq_val_to_serde_value).collect())
        }
        Val::Obj(o) => {
            let o = Rc::try_unwrap(o).unwrap_or_else(|rc| (*rc).clone());
            serde_json::Value::Object(
                o.into_iter()
                    .map(|(k, v)| (k.to_string(), jaq_val_to_serde_value(v)))
                    .collect(),
            )
        }
        Val::Int(i) => serde_json::Value::Number(serde_json::Number::from(i)),
        Val::Float(f) => serde_json::Value::Number(serde_json::Number::from_f64(f).unwrap()),
    }
}

fn jaq_handler(input: serde_json::Value, program: &str) -> anyhow::Result<serde_json::Value> {
    let compiled = jaq_compile(program)?;
    let inputs = RcIter::new(core::iter::empty());
    let out = compiled.run((Ctx::new([], &inputs), Val::from(input)));

    let mut res = Vec::new();
    for v in out {
        let v = v.map_err(|e| anyhow::anyhow!("JQ error: {:?}", e))?;
        res.push(jaq_val_to_serde_value(v));
    }

    Ok(serde_json::Value::Array(res))
}
