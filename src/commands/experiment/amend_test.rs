use std::collections::HashSet;

use crate::{
    error_context::Context,
    experiment::ExperimentMetadata,
    git_driver,
    manager::Manager,
    service::{
        self,
        test_utils::{build_invalid_test_message, resolve_test_name},
    },
};

pub fn invoke(
    _mgr: Manager,
    experiment: ExperimentMetadata,
    test_name: String,
    strategy: String,
    mutation_filters: Vec<String>,
    property_filters: Vec<String>,
) -> anyhow::Result<()> {
    let available_tests = service::experiment::list_tests(&experiment.path)?
        .into_iter()
        .map(|t| t.name)
        .collect::<Vec<_>>();

    if available_tests.is_empty() {
        anyhow::bail!(
            "No tests found in '{}'. Add workloads first (for example: `etna workload add <lang> <workload>`).",
            experiment.path.join("tests").display()
        );
    }

    let resolved_test_name = resolve_test_name(&test_name, &available_tests)
        .with_context(|| build_invalid_test_message(&test_name, &available_tests))?;

    let mut tests = service::experiment::get_test_content(&experiment.path, &resolved_test_name)?;

    let mutation_filters = mutation_filters.into_iter().collect::<HashSet<_>>();
    let property_filters = property_filters.into_iter().collect::<HashSet<_>>();

    let mut selected_tasks = 0usize;
    let mut inserted_missing_strategy = 0usize;
    let mut duplicated_with_new_strategy = 0usize;
    let mut unchanged = 0usize;
    let mut changed = false;

    for test in &mut tests {
        let mutation_match = mutation_filters.is_empty()
            || test
                .mutations
                .iter()
                .any(|mutation| mutation_filters.contains(mutation));
        if !mutation_match {
            continue;
        }

        let mut updated_tasks = test.tasks.clone();

        for (i, task) in test.tasks.iter().enumerate() {
            let property = task.get("property").and_then(|v| v.as_str());
            let property_match = property_filters.is_empty()
                || property.is_some_and(|p| property_filters.contains(p));
            if !property_match {
                continue;
            }
            selected_tasks += 1;

            match task.get("strategy").and_then(|v| v.as_str()) {
                None => {
                    updated_tasks[i].insert(
                        "strategy".to_string(),
                        serde_json::Value::String(strategy.clone()),
                    );
                    inserted_missing_strategy += 1;
                    changed = true;
                }
                Some(existing) if existing == strategy => {
                    unchanged += 1;
                }
                Some(_) => {
                    let mut duplicated = task.clone();
                    duplicated.insert(
                        "strategy".to_string(),
                        serde_json::Value::String(strategy.clone()),
                    );

                    if !updated_tasks.contains(&duplicated) {
                        updated_tasks.push(duplicated);
                        duplicated_with_new_strategy += 1;
                        changed = true;
                    } else {
                        unchanged += 1;
                    }
                }
            }
        }

        test.tasks = updated_tasks;
    }

    if selected_tasks == 0 {
        let mut_hint = if mutation_filters.is_empty() {
            String::new()
        } else {
            format!(
                " (mutation filters: {})",
                mutation_filters.into_iter().collect::<Vec<_>>().join(", ")
            )
        };
        let prop_hint = if property_filters.is_empty() {
            String::new()
        } else {
            format!(
                " (property filters: {})",
                property_filters.into_iter().collect::<Vec<_>>().join(", ")
            )
        };
        anyhow::bail!(
            "No tasks matched in test '{}'{}{}.",
            resolved_test_name,
            mut_hint,
            prop_hint
        );
    }

    if changed {
        service::experiment::save_test(&experiment.path, &resolved_test_name, &tests)?;
        git_driver::commit(
            &experiment.path,
            &format!(
                "amend test '{}' with strategy '{}'",
                resolved_test_name, strategy
            ),
        )
        .with_context(|| {
            format!(
                "Failed to commit amended test '{}'",
                experiment
                    .path
                    .join("tests")
                    .join(&resolved_test_name)
                    .with_extension("json")
                    .display()
            )
        })?;
    }

    tracing::info!(
        "Amended test '{}' with strategy '{}': selected={}, inserted_missing_strategy={}, duplicated_with_new_strategy={}, unchanged={}",
        resolved_test_name,
        strategy,
        selected_tasks,
        inserted_missing_strategy,
        duplicated_with_new_strategy,
        unchanged
    );

    Ok(())
}
