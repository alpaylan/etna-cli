//! Tiny PBT harness used by etna's integration test fixture.
//!
//! Dep-free on purpose: keeps `cargo build --release` cheap for every
//! integration test that spins up a fresh experiment directory. Emits
//! JSON that satisfies `docs/schemas/{campaign-result,input-stream}.schema.json`.

use std::{env, fs, process::ExitCode};

#[derive(Default)]
struct Opts {
    property: String,
    trials: usize,
    inputs: Option<String>,
    counterexample: Option<String>,
    invert: bool,
}

fn parse_opts(args: &[String]) -> Opts {
    let mut opts = Opts::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--property" => {
                opts.property = args.get(i + 1).cloned().unwrap_or_default();
                i += 2;
            }
            "--trials" => {
                opts.trials = args.get(i + 1).and_then(|s| s.parse().ok()).unwrap_or(0);
                i += 2;
            }
            "--inputs" => {
                opts.inputs = args.get(i + 1).cloned();
                i += 2;
            }
            "--counterexample" => {
                opts.counterexample = args.get(i + 1).cloned();
                i += 2;
            }
            "--invert" => {
                opts.invert = true;
                i += 1;
            }
            _ => i += 1,
        }
    }
    opts
}

fn would_fail(property: &str, invert: bool) -> bool {
    let base = property.contains("crash");
    if invert {
        !base
    } else {
        base
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn cmd_solve(args: &[String]) -> String {
    let opts = parse_opts(args);
    let trials = if opts.trials == 0 { 10 } else { opts.trials };
    if would_fail(&opts.property, opts.invert) {
        format!(
            "{{\"status\":\"found_bug\",\"tests\":{},\"discarded\":0,\"time\":\"1ms\",\"counterexample\":\"0\"}}",
            trials
        )
    } else {
        format!(
            "{{\"status\":\"passed\",\"tests\":{},\"discarded\":0,\"time\":\"1ms\"}}",
            trials
        )
    }
}

fn cmd_sample(args: &[String]) -> String {
    let opts = parse_opts(args);
    let trials = if opts.trials == 0 { 10 } else { opts.trials };
    let mut out = String::from("[");
    for i in 0..trials {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"value\":{},\"time\":\"1ms\"}}",
            json_str(&i.to_string())
        ));
    }
    out.push(']');
    out
}

/// Counts inputs in either a JSON-array form (Test mode, written by the driver
/// from `InputSource::Inline`) or an s-expression form `(s1 s2 ...)` (Cross
/// mode consumer input written by the driver from producer samples).
///
/// For JSON arrays we count top-level commas without spinning up a parser —
/// the format is simple enough that tracking string escapes + nesting depth
/// handles every shape the driver emits.
fn count_inputs(content: &str) -> usize {
    let t = content.trim();
    if let Some(body) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        let body = body.trim();
        if body.is_empty() {
            return 0;
        }
        let mut depth: i32 = 0;
        let mut in_str = false;
        let mut escaped = false;
        let mut commas = 0usize;
        for c in body.chars() {
            if in_str {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_str = false;
                }
                continue;
            }
            match c {
                '"' => in_str = true,
                '[' | '{' => depth += 1,
                ']' | '}' => depth -= 1,
                ',' if depth == 0 => commas += 1,
                _ => {}
            }
        }
        return commas + 1;
    }
    t.trim_start_matches('(')
        .trim_end_matches(')')
        .split_whitespace()
        .count()
}

fn cmd_test(args: &[String]) -> Result<String, String> {
    let opts = parse_opts(args);
    let inputs_path = opts
        .inputs
        .as_deref()
        .ok_or_else(|| "missing --inputs".to_string())?;
    let content = fs::read_to_string(inputs_path).map_err(|e| e.to_string())?;
    let count = count_inputs(&content);
    Ok(if would_fail(&opts.property, opts.invert) {
        format!(
            "{{\"status\":\"found_bug\",\"tests\":{},\"discarded\":0,\"time\":\"1ms\",\"counterexample\":\"0\"}}",
            count
        )
    } else {
        format!(
            "{{\"status\":\"passed\",\"tests\":{},\"discarded\":0,\"time\":\"1ms\"}}",
            count
        )
    })
}

fn cmd_shrink(args: &[String]) -> String {
    let opts = parse_opts(args);
    let cex = opts.counterexample.unwrap_or_default();
    format!(
        "{{\"status\":\"found_bug\",\"tests\":1,\"discarded\":0,\"shrinks\":{},\"time\":\"1ms\",\"counterexample\":\"0\"}}",
        cex.len()
    )
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(sub) = args.first() else {
        eprintln!("usage: etna_test_harness <solve|sample|test|shrink> [opts]");
        return ExitCode::FAILURE;
    };

    let result: Result<String, String> = match sub.as_str() {
        "solve" => Ok(cmd_solve(&args[1..])),
        "sample" => Ok(cmd_sample(&args[1..])),
        "test" => cmd_test(&args[1..]),
        "shrink" => Ok(cmd_shrink(&args[1..])),
        other => {
            eprintln!("unknown subcommand: {other}");
            return ExitCode::FAILURE;
        }
    };

    match result {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
