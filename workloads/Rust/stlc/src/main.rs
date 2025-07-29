
use stlc::spec;
use stlc::strategies::bespoke::ExprOpt;

use std::time::Duration;

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() < 3 {
        eprintln!("Usage: {} <tool> <property>", args[0]);
        eprintln!("Available tools: quickcheck");
        eprintln!(
            "Available properties: SinglePreserve, MultiPreserve"
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

    let result = match (tool, property) {
        ("quickcheck", "SinglePreserve") => {
            qc.quicktest(spec::prop_single_preserve as fn(ExprOpt) -> Option<bool>)
        }
        ("quickcheck", "MultiPreserve") => {
            qc.quicktest(spec::prop_multi_preserve as fn(ExprOpt) -> Option<bool>)
        }
        _ => {
            panic!("Unknown tool or property: {} {}", tool, property)
        }
    };


    result.print_status();
}
