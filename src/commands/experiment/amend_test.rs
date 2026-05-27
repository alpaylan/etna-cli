use std::collections::HashMap;

use crate::{
    error_context::Context,
    experiment::{ExperimentMetadata, Test},
    git_driver,
    manager::Manager,
    service::{
        self,
        test_utils::{build_invalid_test_message, resolve_test_name},
    },
};

type Task = HashMap<String, serde_json::Value>;

#[derive(Clone, Debug, PartialEq, Eq)]
enum AmendedStrategy {
    Add { name: String },
    Replace { name: String, replace_with: String },
    Remove { name: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Filter {
    Equal { key: String, value: String },
    NotEqual { key: String, value: String },
}

#[derive(Debug, Default, PartialEq, Eq)]
struct AmendmentStats {
    selected_tasks: usize,
    changed: bool,
}
/// Strategy names (`+STRATEGY` to add, `=STRATEGY` to replace, `-STRATEGY` to remove)
/// Multiple strategies are combined with `;` (e.g. `+strat1;-strat2;=strat3=strat4` applies all three operations in order).
fn parse_strategies(strategies_str: &str) -> anyhow::Result<Vec<AmendedStrategy>> {
    let strategies = strategies_str
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            Ok(match s.chars().next() {
                Some('+') => {
                    let name = s[1..].trim();
                    if name.is_empty() {
                        anyhow::bail!(
                            "Invalid add strategy format: '{}'. Strategy name cannot be empty.",
                            s
                        );
                    }
                    AmendedStrategy::Add {
                        name: name.to_string(),
                    }
                }
                Some('=') => {
                    let parts = s[1..].splitn(2, '=').collect::<Vec<_>>();
                    if parts.len() != 2 {
                        anyhow::bail!(
                            "Invalid replace strategy format: '{}'. Replace strategy must be in the format '=OLD=NEW'.",
                            s
                        );
                    }
                    let name = parts[0].trim();
                    let replace_with = parts[1].trim();
                    if name.is_empty() || replace_with.is_empty() {
                        anyhow::bail!(
                            "Invalid replace strategy format: '{}'. OLD and NEW cannot be empty.",
                            s
                        );
                    }
                    AmendedStrategy::Replace {
                        name: name.to_string(),
                        replace_with: replace_with.to_string(),
                    }
                }
                Some('!') => {
                    let name = s[1..].trim();
                    if name.is_empty() {
                        anyhow::bail!(
                            "Invalid remove strategy format: '{}'. Strategy name cannot be empty.",
                            s
                        );
                    }
                    AmendedStrategy::Remove {
                        name: name.to_string(),
                    }
                }
                _ => anyhow::bail!(
                    "Invalid strategy format: '{}'. Strategy must start with '+', '=', or '!'.",
                    s
                ),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    if strategies.is_empty() {
        anyhow::bail!(
            "No strategies specified. Provide at least one '+STRATEGY', '=OLD=NEW', or '!STRATEGY' operation."
        );
    }

    Ok(strategies)
}

/// Filters to select which tasks to amend (e.g. `workload=foo` or `property=bar`).
/// Multiple filters are combined with `;` (e.g. `workload=foo;property=bar` matches tasks with workload foo AND property bar).
fn parse_filters(filters_str: &str) -> anyhow::Result<Vec<Filter>> {
    filters_str
        .split(';')
        .map(|f| f.trim())
        .filter(|f| !f.is_empty())
        .map(|f| {
            let filter = if let Some((key, value)) = f.split_once("!=") {
                Filter::NotEqual {
                    key: key.trim().to_string(),
                    value: value.trim().to_string(),
                }
            } else if let Some((key, value)) = f.split_once('=') {
                Filter::Equal {
                    key: key.trim().to_string(),
                    value: value.trim().to_string(),
                }
            } else {
                anyhow::bail!(
                    "Invalid filter format: '{}'. Filter must contain '=' or '!='.",
                    f
                )
            };

            match &filter {
                Filter::Equal { key, value } | Filter::NotEqual { key, value } => {
                    if key.is_empty() || value.is_empty() {
                        anyhow::bail!(
                            "Invalid filter format: '{}'. Filter key and value cannot be empty.",
                            f
                        );
                    }
                }
            }

            Ok(filter)
        })
        .collect::<anyhow::Result<Vec<_>>>()
}

fn task_matches_filters(task: &Task, filters: &[Filter]) -> bool {
    filters.iter().all(|filter| match filter {
        Filter::Equal { key, value } => task
            .get(key)
            .and_then(|v| v.as_str())
            .is_some_and(|v| v == value),
        Filter::NotEqual { key, value } => !task
            .get(key)
            .and_then(|v| v.as_str())
            .is_some_and(|v| v == value),
    })
}

fn dedup_tasks(tasks: &mut Vec<Task>) {
    let mut deduped = Vec::with_capacity(tasks.len());
    for task in tasks.drain(..) {
        if !deduped.contains(&task) {
            deduped.push(task);
        }
    }
    *tasks = deduped;
}

fn apply_amendments(
    tests: &mut [Test],
    strategies: &[AmendedStrategy],
    filters: &[Filter],
) -> AmendmentStats {
    let mut stats = AmendmentStats::default();

    for test in tests {
        let before_tasks = test.tasks.clone();
        let selected_before = stats.selected_tasks;

        for strategy in strategies {
            match strategy {
                AmendedStrategy::Add { name } => {
                    let mut operation_filters = filters.to_vec();
                    operation_filters.push(Filter::NotEqual {
                        key: "strategy".to_string(),
                        value: name.clone(),
                    });

                    let new_tasks = test
                        .tasks
                        .iter()
                        .filter(|task| task_matches_filters(task, &operation_filters))
                        .map(|task| {
                            let mut new_task = task.clone();
                            new_task.insert(
                                "strategy".to_string(),
                                serde_json::Value::String(name.clone()),
                            );
                            new_task
                        })
                        .collect::<Vec<_>>();

                    stats.selected_tasks += new_tasks.len();
                    test.tasks.extend(new_tasks);
                }
                AmendedStrategy::Replace { name, replace_with } => {
                    let mut operation_filters = filters.to_vec();
                    operation_filters.push(Filter::Equal {
                        key: "strategy".to_string(),
                        value: name.clone(),
                    });

                    for task in &mut test.tasks {
                        if task_matches_filters(task, &operation_filters) {
                            stats.selected_tasks += 1;
                            task.insert(
                                "strategy".to_string(),
                                serde_json::Value::String(replace_with.clone()),
                            );
                        }
                    }
                }
                AmendedStrategy::Remove { name } => {
                    let mut operation_filters = filters.to_vec();
                    operation_filters.push(Filter::Equal {
                        key: "strategy".to_string(),
                        value: name.clone(),
                    });

                    test.tasks.retain(|task| {
                        let matched = task_matches_filters(task, &operation_filters);
                        if matched {
                            stats.selected_tasks += 1;
                        }
                        !matched
                    });
                }
            }
        }

        if stats.selected_tasks > selected_before {
            dedup_tasks(&mut test.tasks);
        }
        stats.changed |= test.tasks != before_tasks;
    }

    stats
}

fn describe_strategy_match(strategy: &AmendedStrategy) -> String {
    match strategy {
        AmendedStrategy::Add { name } => format!(" with strategy != '{}'", name),
        AmendedStrategy::Replace { name, .. } => format!(" with strategy == '{}'", name),
        AmendedStrategy::Remove { name } => format!(" with strategy == '{}'", name),
    }
}

fn describe_filter(filter: &Filter) -> String {
    match filter {
        Filter::Equal { key, value } => format!(" with {} == '{}'", key, value),
        Filter::NotEqual { key, value } => format!(" with {} != '{}'", key, value),
    }
}

pub fn invoke(
    _mgr: Manager,
    experiment: ExperimentMetadata,
    test_name: String,
    strategies_str: String,
    filters_str: String,
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
    let strategies = parse_strategies(&strategies_str)?;
    let filters = parse_filters(&filters_str)?;
    let stats = apply_amendments(&mut tests, &strategies, &filters);

    if stats.selected_tasks == 0 {
        anyhow::bail!(
            "No tasks matched in test '{}'{}{}.",
            resolved_test_name,
            strategies
                .iter()
                .map(describe_strategy_match)
                .collect::<Vec<_>>()
                .join(" and"),
            filters
                .iter()
                .map(describe_filter)
                .collect::<Vec<_>>()
                .join(" and")
        );
    }

    if stats.changed {
        service::experiment::save_test(&experiment.path, &resolved_test_name, &tests)?;
        git_driver::commit(
            &experiment.path,
            &format!(
                "amend test '{}' with strategies '{}' and filters '{}'",
                resolved_test_name, strategies_str, filters_str
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
        "Amended test '{}' with strategies '{}' and filters '{}': selected={}, changed={}",
        resolved_test_name,
        strategies_str,
        filters_str,
        stats.selected_tasks,
        stats.changed
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::experiment::Mode;

    fn task(fields: &[(&str, serde_json::Value)]) -> Task {
        fields
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    fn prop_strategy(property: &str, strategy: &str) -> Task {
        task(&[("property", json!(property)), ("strategy", json!(strategy))])
    }

    fn prop_strategy_extra(
        property: &str,
        strategy: &str,
        key: &str,
        value: serde_json::Value,
    ) -> Task {
        task(&[
            ("property", json!(property)),
            ("strategy", json!(strategy)),
            (key, value),
        ])
    }

    fn test_with_tasks(tasks: Vec<Task>) -> Test {
        Test {
            workload: "bar".to_string(),
            trials: 10,
            timeout: 60.0,
            mutations: vec!["m1".to_string()],
            mode: Mode::Solve,
            params: None,
            tasks,
        }
    }

    fn apply(tests: &mut [Test], strategies: &str, filters: &str) -> AmendmentStats {
        let strategies = parse_strategies(strategies).unwrap();
        let filters = parse_filters(filters).unwrap();
        apply_amendments(tests, &strategies, &filters)
    }

    fn strategies(tasks: &[Task]) -> Vec<&str> {
        tasks
            .iter()
            .filter_map(|task| task.get("strategy")?.as_str())
            .collect()
    }

    fn properties(tasks: &[Task]) -> Vec<Option<&str>> {
        tasks
            .iter()
            .map(|task| task.get("property").and_then(|v| v.as_str()))
            .collect()
    }

    #[test]
    fn add_new_strategy_duplicates_matching_tasks() {
        let mut tests = vec![test_with_tasks(vec![prop_strategy("p1", "s1")])];
        let stats = apply(&mut tests, "+s2", "property=p1");

        assert_eq!(
            stats,
            AmendmentStats {
                selected_tasks: 1,
                changed: true
            }
        );
        assert_eq!(strategies(&tests[0].tasks), vec!["s1", "s2"]);
    }

    #[test]
    fn add_existing_strategy_does_not_duplicate_existing_target() {
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("p1", "s1"),
            prop_strategy("p1", "s2"),
        ])];
        let stats = apply(&mut tests, "+s2", "property=p1");

        assert_eq!(
            stats,
            AmendmentStats {
                selected_tasks: 1,
                changed: false
            }
        );
        assert_eq!(tests[0].tasks.len(), 2);
    }

    #[test]
    fn add_is_idempotent_when_repeated() {
        let mut tests = vec![test_with_tasks(vec![prop_strategy("p1", "s1")])];
        assert!(apply(&mut tests, "+s2", "property=p1").changed);

        let stats = apply(&mut tests, "+s2", "property=p1");
        assert_eq!(
            stats,
            AmendmentStats {
                selected_tasks: 1,
                changed: false
            }
        );
        assert_eq!(tests[0].tasks.len(), 2);
    }

    #[test]
    fn replace_changes_only_old_strategy() {
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("p1", "s1"),
            prop_strategy("p1", "s2"),
        ])];
        let stats = apply(&mut tests, "=s1=s3", "");

        assert_eq!(
            stats,
            AmendmentStats {
                selected_tasks: 1,
                changed: true
            }
        );
        assert_eq!(strategies(&tests[0].tasks), vec!["s3", "s2"]);
    }

    #[test]
    fn replace_preserves_other_fields() {
        let mut tests = vec![test_with_tasks(vec![prop_strategy_extra(
            "p1",
            "s1",
            "witnesses",
            json!(["w1"]),
        )])];
        apply(&mut tests, "=s1=s2", "");

        assert_eq!(tests[0].tasks[0].get("property"), Some(&json!("p1")));
        assert_eq!(tests[0].tasks[0].get("witnesses"), Some(&json!(["w1"])));
        assert_eq!(tests[0].tasks[0].get("strategy"), Some(&json!("s2")));
    }

    #[test]
    fn remove_strategy_removes_only_that_strategy() {
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("p1", "s1"),
            prop_strategy("p1", "s2"),
        ])];
        let stats = apply(&mut tests, "!s1", "");

        assert_eq!(
            stats,
            AmendmentStats {
                selected_tasks: 1,
                changed: true
            }
        );
        assert_eq!(strategies(&tests[0].tasks), vec!["s2"]);
    }

    #[test]
    fn remove_missing_strategy_selects_no_tasks() {
        let mut tests = vec![test_with_tasks(vec![prop_strategy("p1", "s1")])];
        let stats = apply(&mut tests, "!missing", "");

        assert_eq!(
            stats,
            AmendmentStats {
                selected_tasks: 0,
                changed: false
            }
        );
        assert_eq!(tests[0].tasks.len(), 1);
    }

    #[test]
    fn equality_filter_matches_exact_string() {
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("foo", "s1"),
            prop_strategy("foobar", "s1"),
        ])];
        apply(&mut tests, "+s2", "property=foo");

        assert_eq!(
            properties(&tests[0].tasks),
            vec![Some("foo"), Some("foobar"), Some("foo")]
        );
    }

    #[test]
    fn not_equal_filter_matches_other_and_missing() {
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("foo", "s1"),
            prop_strategy("bar", "s1"),
            task(&[("strategy", json!("s1"))]),
        ])];
        apply(&mut tests, "+s2", "property!=foo");

        assert_eq!(tests[0].tasks.len(), 5);
        assert_eq!(
            strategies(&tests[0].tasks),
            vec!["s1", "s1", "s1", "s2", "s2"]
        );
    }

    #[test]
    fn multiple_filters_are_and() {
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("foo", "s1"),
            prop_strategy("foo", "s2"),
            prop_strategy("bar", "s1"),
        ])];
        apply(&mut tests, "+s3", "property=foo;strategy=s1");

        assert_eq!(strategies(&tests[0].tasks), vec!["s1", "s2", "s1", "s3"]);
        assert_eq!(
            properties(&tests[0].tasks),
            vec![Some("foo"), Some("foo"), Some("bar"), Some("foo")]
        );
    }

    #[test]
    fn mixed_equal_and_not_equal_filters_are_and() {
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("foo", "s1"),
            prop_strategy("foo", "s2"),
            prop_strategy("bar", "s1"),
        ])];
        apply(&mut tests, "+s3", "property=foo;strategy!=s1");

        assert_eq!(strategies(&tests[0].tasks), vec!["s1", "s2", "s1", "s3"]);
        assert_eq!(
            properties(&tests[0].tasks),
            vec![Some("foo"), Some("foo"), Some("bar"), Some("foo")]
        );
    }

    #[test]
    fn missing_key_equal_does_not_match() {
        let mut tests = vec![test_with_tasks(vec![task(&[("strategy", json!("s1"))])])];
        let stats = apply(&mut tests, "+s2", "property=foo");

        assert_eq!(
            stats,
            AmendmentStats {
                selected_tasks: 0,
                changed: false
            }
        );
    }

    #[test]
    fn missing_key_not_equal_matches() {
        let mut tests = vec![test_with_tasks(vec![task(&[("strategy", json!("s1"))])])];
        let stats = apply(&mut tests, "+s2", "property!=foo");

        assert_eq!(
            stats,
            AmendmentStats {
                selected_tasks: 1,
                changed: true
            }
        );
        assert_eq!(strategies(&tests[0].tasks), vec!["s1", "s2"]);
    }

    #[test]
    fn remove_with_equal_filter_removes_only_both() {
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("p1", "s1"),
            prop_strategy("p2", "s1"),
            prop_strategy("p1", "s2"),
        ])];
        apply(&mut tests, "!s1", "property=p1");

        assert_eq!(
            tests[0].tasks,
            vec![prop_strategy("p2", "s1"), prop_strategy("p1", "s2")]
        );
    }

    #[test]
    fn remove_with_not_equal_filter_includes_missing_key() {
        let missing_property = task(&[("strategy", json!("s1"))]);
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("p1", "s1"),
            prop_strategy("p2", "s1"),
            missing_property,
        ])];
        apply(&mut tests, "!s1", "property!=p1");

        assert_eq!(tests[0].tasks, vec![prop_strategy("p1", "s1")]);
    }

    #[test]
    fn remove_keeps_same_property_different_strategy() {
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("p1", "s1"),
            prop_strategy("p1", "s2"),
        ])];
        apply(&mut tests, "!s1", "property=p1");

        assert_eq!(tests[0].tasks, vec![prop_strategy("p1", "s2")]);
    }

    #[test]
    fn remove_equal_filter_keeps_missing_filtered_key() {
        let missing_property = task(&[("strategy", json!("s1"))]);
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("p1", "s1"),
            missing_property.clone(),
        ])];
        apply(&mut tests, "!s1", "property=p1");

        assert_eq!(tests[0].tasks, vec![missing_property]);
    }

    #[test]
    fn add_then_remove_applies_in_order() {
        let mut tests = vec![test_with_tasks(vec![prop_strategy("p1", "s1")])];
        apply(&mut tests, "+s2;!s1", "");

        assert_eq!(tests[0].tasks, vec![prop_strategy("p1", "s2")]);
    }

    #[test]
    fn remove_then_add_order_differs_from_add_then_remove() {
        let mut add_then_remove = vec![test_with_tasks(vec![prop_strategy("p1", "s1")])];
        let mut remove_then_add = add_then_remove.clone();

        apply(&mut add_then_remove, "+s2;!s1", "");
        apply(&mut remove_then_add, "!s1;+s2", "");

        assert_eq!(add_then_remove[0].tasks, vec![prop_strategy("p1", "s2")]);
        assert!(remove_then_add[0].tasks.is_empty());
    }

    #[test]
    fn replace_then_remove_can_remove_replaced_tasks() {
        let mut tests = vec![test_with_tasks(vec![prop_strategy("p1", "s1")])];
        apply(&mut tests, "=s1=s2;!s2", "");

        assert!(tests[0].tasks.is_empty());
    }

    #[test]
    fn add_then_replace_affects_new_tasks() {
        let mut tests = vec![test_with_tasks(vec![prop_strategy("p1", "s1")])];
        apply(&mut tests, "+s2;=s2=s3", "");

        assert_eq!(strategies(&tests[0].tasks), vec!["s1", "s3"]);
    }

    #[test]
    fn empty_strategy_chunks_are_ignored() {
        let strategies = parse_strategies("+s1;;!s2").unwrap();

        assert_eq!(
            strategies,
            vec![
                AmendedStrategy::Add {
                    name: "s1".to_string()
                },
                AmendedStrategy::Remove {
                    name: "s2".to_string()
                },
            ]
        );
    }

    #[test]
    fn strategy_without_prefix_errors() {
        assert!(parse_strategies("s1").is_err());
    }

    #[test]
    fn replace_without_new_errors() {
        assert!(parse_strategies("=s1").is_err());
    }

    #[test]
    fn empty_replacement_pieces_error() {
        assert!(parse_strategies("=s1=").is_err());
        assert!(parse_strategies("==s2").is_err());
    }

    #[test]
    fn filter_without_operator_errors() {
        assert!(parse_filters("property").is_err());
    }

    #[test]
    fn empty_filter_string_means_no_extra_filters() {
        assert_eq!(parse_filters("").unwrap(), Vec::<Filter>::new());
    }

    #[test]
    fn empty_strategies_string_errors() {
        assert!(parse_strategies("").is_err());
    }

    #[test]
    fn numeric_json_not_matched_by_string_filter() {
        let mut tests = vec![test_with_tasks(vec![task(&[
            ("property", json!("p1")),
            ("strategy", json!("s1")),
            ("trials", json!(10)),
        ])])];
        let stats = apply(&mut tests, "+s2", "trials=10");

        assert_eq!(
            stats,
            AmendmentStats {
                selected_tasks: 0,
                changed: false
            }
        );
    }

    #[test]
    fn boolean_json_not_matched_by_string_filter() {
        let mut tests = vec![test_with_tasks(vec![task(&[
            ("property", json!("p1")),
            ("strategy", json!("s1")),
            ("flag", json!(true)),
        ])])];
        let stats = apply(&mut tests, "+s2", "flag=true");

        assert_eq!(
            stats,
            AmendmentStats {
                selected_tasks: 0,
                changed: false
            }
        );
    }

    #[test]
    fn string_only_filter_contract_matches_strings() {
        let mut tests = vec![test_with_tasks(vec![prop_strategy_extra(
            "p1",
            "s1",
            "flag",
            json!("true"),
        )])];
        let stats = apply(&mut tests, "+s2", "flag=true");

        assert_eq!(
            stats,
            AmendmentStats {
                selected_tasks: 1,
                changed: true
            }
        );
        assert_eq!(strategies(&tests[0].tasks), vec!["s1", "s2"]);
    }

    #[test]
    fn order_preserved_with_appended_added_tasks() {
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("p1", "s1"),
            prop_strategy("p2", "s1"),
        ])];
        apply(&mut tests, "+s2", "");

        assert_eq!(
            tests[0].tasks,
            vec![
                prop_strategy("p1", "s1"),
                prop_strategy("p2", "s1"),
                prop_strategy("p1", "s2"),
                prop_strategy("p2", "s2"),
            ]
        );
    }

    #[test]
    fn exact_duplicate_task_maps_are_removed() {
        let duplicate = prop_strategy("p1", "s1");
        let mut tests = vec![test_with_tasks(vec![
            duplicate.clone(),
            duplicate,
            prop_strategy("p2", "s1"),
        ])];
        apply(&mut tests, "+s2", "property=p2");

        assert_eq!(
            tests[0].tasks,
            vec![
                prop_strategy("p1", "s1"),
                prop_strategy("p2", "s1"),
                prop_strategy("p2", "s2"),
            ]
        );
    }

    #[test]
    fn dedup_does_not_collapse_tasks_that_differ_by_non_strategy_field() {
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy_extra("p1", "s1", "case", json!("a")),
            prop_strategy_extra("p1", "s1", "case", json!("b")),
        ])];
        apply(&mut tests, "+s2", "");

        assert_eq!(tests[0].tasks.len(), 4);
    }

    #[test]
    fn no_op_duplicate_add_reports_unchanged() {
        let mut tests = vec![test_with_tasks(vec![
            prop_strategy("p1", "s1"),
            prop_strategy("p1", "s2"),
        ])];
        let stats = apply(&mut tests, "+s2", "property=p1");

        assert_eq!(
            stats,
            AmendmentStats {
                selected_tasks: 1,
                changed: false
            }
        );
    }
}
