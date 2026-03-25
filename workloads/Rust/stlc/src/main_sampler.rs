use hegel::{HealthCheck, Hegel, Settings, Verbosity};
use proptest::{
    strategy::{Strategy, ValueTree},
    test_runner::{Config, TestRunner},
};
use std::time::{Duration, Instant};
use stlc::{spec, strategies::bespoke::ExprOpt};

fn sample_hegel(property: &str, tests: u64) -> Vec<(Duration, String)> {
    let mut results = Vec::with_capacity(tests as usize);
    let settings = Settings::new()
        .test_cases(tests)
        .verbosity(Verbosity::Quiet)
        .database(None)
        .suppress_health_check(HealthCheck::all());

    Hegel::new(|tc| {
        let start = Instant::now();
        let Some((sample, _result)) = stlc::strategies::hegel::draw_case(property, &tc) else {
            panic!("Unknown property: {}", property);
        };
        results.push((start.elapsed(), sample));
    })
    .settings(settings)
    .run();

    results
}

fn sample_proptest(property: &str, tests: u64) -> Vec<(Duration, String)> {
    let Some(strategy) = stlc::strategies::proptest::strategy_for(property) else {
        panic!("Unknown property: {}", property);
    };

    let cases = tests.min(u64::from(u32::MAX)) as u32;
    let mut runner = TestRunner::new(Config {
        cases,
        max_global_rejects: cases.saturating_mul(20),
        failure_persistence: None,
        ..Config::default()
    });

    let mut results = Vec::with_capacity(cases as usize);
    for _ in 0..cases {
        let start = Instant::now();
        let value = strategy
            .new_tree(&mut runner)
            .expect("Failed to generate proptest sample");
        let (sample, _result) = value.current();
        results.push((start.elapsed(), sample));
    }

    results
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 4 {
        eprintln!("Usage: {} <tool> <property> <tests>", args[0]);
        eprintln!("Available tools: quickcheck, hegel, proptest");
        eprintln!(
            "For available properties, check https://github.com/alpaylan/etna-cli/blob/main/docs/workloads/stlc.md"
        );
        return;
    }
    let tool = args[1].as_str();
    let property = args[2].as_str();
    let tests = args[3].as_str();

    let num_tests = tests
        .parse::<u64>()
        .expect(format!("Failed to parse number of tests: '{}'", tests).as_str());
    let mut qc = quickcheck::QuickCheck::new()
        .tests(num_tests)
        .max_tests(num_tests * 2)
        .max_time(std::time::Duration::from_secs(1));

    let result: Vec<(Duration, String)> = match tool {
        "quickcheck" => match property {
            "SinglePreserve" => {
                qc.quicksample(spec::prop_single_preserve as fn(ExprOpt) -> Option<bool>)
            }
            "MultiPreserve" => {
                qc.quicksample(spec::prop_multi_preserve as fn(ExprOpt) -> Option<bool>)
            }
            _ => panic!("Unknown property: {}", property),
        },
        "hegel" => sample_hegel(property, num_tests),
        "proptest" => sample_proptest(property, num_tests),
        _ => panic!("Unknown tool: {}", tool),
    };

    let mut results = Vec::<serde_json::Value>::new();

    for (duration, element) in result {
        let mut object = serde_json::Map::new();
        object.insert(
            "time".to_string(),
            serde_json::Value::String(format!("{}ns", duration.as_nanos())),
        );
        object.insert(
            "value".to_string(),
            serde_json::Value::String(element.to_string()),
        );
        results.push(serde_json::Value::Object(object));
    }

    let results = serde_json::Value::Array(results);

    let output = serde_json::to_string(&results).expect("Failed to serialize results to JSON");

    println!("{}", output);
}
