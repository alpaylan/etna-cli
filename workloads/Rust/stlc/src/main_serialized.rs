use std::{path::Path, process::ExitCode};

use etna_rs_utils::{SamplingResult, Status};
use stlc::{parser, spec, strategies::bespoke::ExprOpt};

fn sample(property: String, tests: &str) -> SamplingResult {
    let mut discarded = 0;
    let mut passed = 0;
    match property.as_str() {
        "SinglePreserve" => {
            let Ok(tests) = parser::parse(&tests) else {
                return SamplingResult {
                    property,
                    status: Status::Aborted("failed to parse tests".to_string()),
                    tests: 0,
                    passed,
                    discarded,
                };
            };

            for e in tests.into_iter() {
                match spec::prop_single_preserve(ExprOpt(Some(e.clone()))) {
                    None => {
                        discarded += 1;
                    }
                    Some(true) => {
                        passed += 1;
                    }
                    Some(false) => {
                        return SamplingResult {
                            property,
                            status: Status::FoundBug(format!("{}", e)),
                            tests: passed + discarded,
                            passed,
                            discarded,
                        };
                    }
                }
            }
        }
        "MultiPreserve" => {
            let Ok(tests) = parser::parse(&tests) else {
                return SamplingResult {
                    property,
                    status: Status::Aborted("failed to parse tests".to_string()),
                    tests: 0,
                    passed,
                    discarded,
                };
            };

            for e in tests.into_iter() {
                match spec::prop_multi_preserve(ExprOpt(Some(e.clone()))) {
                    None => {
                        discarded += 1;
                    }
                    Some(true) => {
                        passed += 1;
                    }
                    Some(false) => {
                        return SamplingResult {
                            property,
                            status: Status::FoundBug(format!("{}", e)),
                            tests: passed + discarded,
                            passed,
                            discarded,
                        };
                    }
                }
            }
        }
        _ => {
            return SamplingResult {
                status: Status::Aborted(format!("Unknown property: {}", property)),
                property,
                tests: 0,
                passed,
                discarded,
            };
        }
    }
    return SamplingResult {
        property,
        status: Status::Finished,
        tests: passed + discarded,
        passed,
        discarded,
    };
}

fn main() -> ExitCode {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 3 {
        eprintln!("Usage: {} <tests> <property>", args[0]);
        eprintln!("Tests should be an s-expression that is a list of test cases.");
        eprintln!(
            "For available properties, check https://github.com/alpaylan/etna-cli/blob/main/docs/workloads/stlc.md"
        );
        return ExitCode::FAILURE;
    }
    let tests = args[1].as_str();
    let property = args[2].as_str();

    let tests = if Path::new(tests).exists() {
        std::fs::read_to_string(tests).expect("Failed to read tests file")
    } else {
        tests.to_string()
    };

    let result = sample(property.to_string(), &tests);

    println!("{}", result);

    match result.status {
        Status::Finished => ExitCode::SUCCESS,
        Status::FoundBug(_) | Status::Aborted(_) => ExitCode::FAILURE,
    }
}
