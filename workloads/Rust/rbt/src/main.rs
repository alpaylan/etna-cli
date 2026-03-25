use rbt::implementation::Tree;
use rbt::spec;

use hegel::{HealthCheck, Hegel, Settings};
use proptest::test_runner::{Config, TestCaseError, TestRunner};
use std::any::Any;
use std::panic::{self, AssertUnwindSafe};
use std::time::{Duration, Instant};

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(msg) = payload.downcast_ref::<&str>() {
        (*msg).to_string()
    } else if let Some(msg) = payload.downcast_ref::<String>() {
        msg.clone()
    } else {
        "panic without message".to_string()
    }
}

fn emit_result(
    status: &str,
    tests: u64,
    discards: u64,
    counterexample: Option<String>,
    error: Option<String>,
    started: Instant,
) {
    let elapsed = format!("{}ns", started.elapsed().as_nanos());
    let result = serde_json::json!({
        "counterexample": counterexample,
        "discards": discards,
        "error": error,
        "execution_time": null,
        "generation_time": null,
        "shrinking_time": null,
        "status": status,
        "tests": tests,
        "time": elapsed,
    });
    println!("{}", result);
}

fn run_hegel(property: &str) {
    use std::cell::{Cell, RefCell};

    let started = Instant::now();
    let draws = Cell::new(0_u64);
    let discards = Cell::new(0_u64);
    let failing_sample = RefCell::new(None::<String>);

    let num_tests = 200_000_000;
    let settings = Settings::new()
        .test_cases(num_tests)
        .suppress_health_check([HealthCheck::FilterTooMuch]);

    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        Hegel::new(|tc| {
            draws.set(draws.get().saturating_add(1));
            let Some((sample, result)) = rbt::strategies::hegel::draw_case(property, &tc) else {
                panic!("Unknown property: {}", property);
            };
            match result {
                Some(true) => {}
                Some(false) => {
                    failing_sample.replace(Some(sample));
                    panic!("Property failed: {}", property)
                }
                None => {
                    discards.set(discards.get().saturating_add(1));
                    tc.assume(false)
                }
            }
        })
        .settings(settings)
        .run();
    }));
    panic::set_hook(previous_hook);

    match outcome {
        Ok(()) => emit_result("passed", draws.get(), discards.get(), None, None, started),
        Err(payload) => {
            let msg = panic_message(payload);
            let failed = msg.contains("Property failed") || msg.contains("Property test failed");
            if failed {
                let counterexample = failing_sample.borrow_mut().take().or(Some(msg));
                emit_result(
                    "failed",
                    draws.get(),
                    discards.get(),
                    counterexample,
                    None,
                    started,
                );
            } else {
                emit_result("aborted", draws.get(), discards.get(), None, Some(msg), started);
            }
        }
    }
}

fn run_proptest(property: &str) {
    let started = Instant::now();
    let Some(strategy) = rbt::strategies::proptest::strategy_for(property) else {
        emit_result(
            "aborted",
            0,
            0,
            None,
            Some(format!("Unknown property: {}", property)),
            started,
        );
        return;
    };

    let num_tests = 200_000_000_u32;
    let mut runner = TestRunner::new(Config {
        cases: num_tests,
        max_global_rejects: num_tests.saturating_mul(20),
        ..Config::default()
    });
    let tests = std::cell::Cell::new(0_u64);
    let discards = std::cell::Cell::new(0_u64);

    let result = runner.run(&strategy, |(_sample, outcome)| match outcome {
        Some(true) => {
            tests.set(tests.get().saturating_add(1));
            Ok(())
        }
        Some(false) => Err(TestCaseError::fail(format!(
            "Property failed: {}",
            property
        ))),
        None => {
            tests.set(tests.get().saturating_add(1));
            discards.set(discards.get().saturating_add(1));
            Err(TestCaseError::reject("discarded by property"))
        }
    });

    match result {
        Ok(()) => emit_result("passed", tests.get(), discards.get(), None, None, started),
        Err(err) => {
            let msg = err.to_string();
            let aborted = msg.to_ascii_lowercase().contains("reject");
            if aborted {
                emit_result("aborted", tests.get(), discards.get(), None, Some(msg), started);
            } else {
                emit_result("failed", tests.get(), discards.get(), Some(msg), None, started);
            }
        }
    }
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 3 {
        eprintln!("Usage: {} <tool> <property>", args[0]);
        eprintln!("Available tools: quickcheck, hegel, proptest");
        eprintln!(
            "For available properties, check https://github.com/alpaylan/etna-cli/blob/main/docs/workloads/rbt.md"
        );
        return;
    }
    let tool = args[1].as_str();
    let property = args[2].as_str();

    let num_tests = 200_000_000;
    let mut qc = quickcheck::QuickCheck::new()
        .tests(num_tests)
        .max_tests(num_tests * 2)
        .max_time(Duration::from_secs(60 * 60));

    match tool {
        "quickcheck" => {
            let result = match property {
                "InsertValid" => {
                    qc.quicktest(spec::prop_insert_valid as fn(Tree, i32, i32) -> Option<bool>)
                }
                "DeleteValid" => {
                    qc.quicktest(spec::prop_delete_valid as fn(Tree, i32) -> Option<bool>)
                }
                "InsertPost" => {
                    qc.quicktest(spec::prop_insert_post as fn(Tree, i32, i32, i32) -> Option<bool>)
                }
                "DeletePost" => {
                    qc.quicktest(spec::prop_delete_post as fn(Tree, i32, i32) -> Option<bool>)
                }
                "InsertModel" => {
                    qc.quicktest(spec::prop_insert_model as fn(Tree, i32, i32) -> Option<bool>)
                }
                "DeleteModel" => {
                    qc.quicktest(spec::prop_delete_model as fn(Tree, i32) -> Option<bool>)
                }
                "InsertInsert" => qc.quicktest(
                    spec::prop_insert_insert as fn(Tree, i32, i32, i32, i32) -> Option<bool>,
                ),
                "InsertDelete" => qc
                    .quicktest(spec::prop_insert_delete as fn(Tree, i32, i32, i32) -> Option<bool>),
                "DeleteInsert" => qc
                    .quicktest(spec::prop_delete_insert as fn(Tree, i32, i32, i32) -> Option<bool>),
                "DeleteDelete" => {
                    qc.quicktest(spec::prop_delete_delete as fn(Tree, i32, i32) -> Option<bool>)
                }
                _ => panic!("Unknown property: {}", property),
            };
            result.print_status();
        }
        "hegel" => run_hegel(property),
        "proptest" => run_proptest(property),
        _ => panic!("Unknown tool: {}", tool),
    }
}
