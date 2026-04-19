use std::{collections::HashMap, os::unix::fs::PermissionsExt as _, path::PathBuf};

use crate::{
    git_driver,
    manager::Manager,
    workload::{Step, Steps},
};

use minijinja::{path_loader, Environment};
use regex::Regex;
use serde::Serialize;
use serde_json::{self as json, Value as JsonValue};
use std::collections::HashSet;

/// Detect occurrences like `$var` or `${var}` in an arbitrary string.
fn contains_var_token(s: &str, var: &str, re_cache: &mut Vec<(String, Regex)>) -> bool {
    // Cache compiled regex per variable to avoid recompiling
    if let Some((_, re)) = re_cache.iter().find(|(v, _)| v == var) {
        return re.is_match(s);
    }
    let pat = format!(r"\$\{{?\b{}\b\}}?", regex::escape(var));
    let re = Regex::new(&pat).unwrap();
    re_cache.push((var.to_string(), re));
    re_cache.last().unwrap().1.is_match(s)
}

/// Detect occurences of `${var}` for an arbitrary unknown `var`.
fn contains_unknown_variable(s: &str) -> Option<&str> {
    let re = Regex::new(r"\$\{(\w+)\}").unwrap();
    if let Some(caps) = re.captures(s) {
        if let Some(m) = caps.get(1) {
            return Some(m.as_str());
        }
    }
    None
}
/// Recursively walk a Step to collect used variable names (from CANDIDATE_VARIABLES).
fn collect_used_from_step<'a>(
    step: &'a Step,
    used: &mut HashSet<&'a str>,
    re_cache: &mut Vec<(String, Regex)>,
) {
    match step {
        Step::Command {
            command,
            args,
            run_at,
            mitigation,
            env,
        } => {
            for &v in &CANDIDATE_VARIABLES {
                tracing::trace!("Checking command '{}' for var '{}'", command, v);
                if contains_var_token(command, v, re_cache) {
                    tracing::trace!("Found usage of var '{}'", v);
                    used.insert(v);
                }
            }
            // Also check for any unknown variable usage
            if let Some(unknown) = contains_unknown_variable(command) {
                tracing::debug!(
                    "command '{}' contains unknown variable '${{{}}}'",
                    command,
                    unknown
                );
                used.insert(unknown);
            }
            // args
            for a in args {
                for &v in &CANDIDATE_VARIABLES {
                    if contains_var_token(a, v, re_cache) {
                        used.insert(v);
                    }
                }
                if let Some(unknown) = contains_unknown_variable(a) {
                    tracing::debug!("arg '{}' contains unknown variable '${{{}}}'", a, unknown);
                    used.insert(unknown);
                }
            }

            // run_at
            if let Some(dir) = run_at {
                for &v in &CANDIDATE_VARIABLES {
                    if contains_var_token(dir, v, re_cache) {
                        used.insert(v);
                    }
                }
                if let Some(unknown) = contains_unknown_variable(dir) {
                    tracing::debug!(
                        "run_at '{}' contains unknown variable '${{{}}}'",
                        dir,
                        unknown
                    );
                    used.insert(unknown);
                }
            }
            // mitigation text (might reference vars)
            if let Some(m) = mitigation {
                for &v in &CANDIDATE_VARIABLES {
                    if contains_var_token(m, v, re_cache) {
                        used.insert(v);
                    }
                }
                if let Some(unknown) = contains_unknown_variable(m) {
                    tracing::debug!(
                        "mitigation '{}' contains unknown variable '${{{}}}'",
                        m,
                        unknown
                    );
                    used.insert(unknown);
                }
            }
            // env values (keys are literal names; values may reference vars)
            for val in env.values() {
                for &v in &CANDIDATE_VARIABLES {
                    if contains_var_token(val, v, re_cache) {
                        used.insert(v);
                    }
                }
                if let Some(unknown) = contains_unknown_variable(val) {
                    tracing::debug!(
                        "env value '{}' contains unknown variable '${{{}}}'",
                        val,
                        unknown
                    );
                    used.insert(unknown);
                }
            }
        }
        Step::Match { value, options } => {
            // The match "value" is directly a variable name in your to_bash()
            used.insert(value.as_str());
            // Recurse into branches
            for st in options.values() {
                collect_used_from_step(st, used, re_cache);
            }
        }
    }
}

/// Scan the entire Steps to find all actually-used candidate variables.
fn collect_used_variables(cfg: &Steps) -> HashSet<&str> {
    let mut used: HashSet<&str> = HashSet::new();
    let mut re_cache: Vec<(String, Regex)> = Vec::new();

    for st in &cfg.setup {
        collect_used_from_step(st, &mut used, &mut re_cache);
    }

    for st in &cfg.build {
        collect_used_from_step(st, &mut used, &mut re_cache);
    }

    for steps in cfg.capabilities.values() {
        for st in steps {
            collect_used_from_step(st, &mut used, &mut re_cache);
        }
    }

    used
}

#[derive(Serialize)]
struct VarCtx<'a> {
    name: &'a str,
}

#[derive(Serialize)]
struct TemplateCtx<'a> {
    vars: Vec<VarCtx<'a>>,
    setup: Vec<String>,
    build: Vec<String>,
    test: Vec<String>,
}

/// The path should point to a valid run configuration
/// This command produces a bash script `steps.sh` that executes the steps defined in the configuration
pub fn invoke(mgr: Manager, path: Option<PathBuf>) -> anyhow::Result<()> {
    tracing::info!("Generating bash script from configuration...");
    let path = path.unwrap_or_else(|| std::env::current_dir().unwrap().join("steps.json"));

    // Load full steps to access tags for !expansion
    let steps_path = if path.is_dir() {
        path.join("steps.json")
    } else {
        path.clone()
    };
    let steps_str = std::fs::read_to_string(&steps_path)?;
    let steps_json: JsonValue = json::from_str(&steps_str)?;

    // Build steps from JSON, same as previous behavior
    let steps: Steps = Steps::from_value(&steps_json)?;

    // Use empty params so $vars remain for runtime CLI; only !tags expand here
    let params: HashMap<String, String> = HashMap::new();

    // Expand and pre-render the step commands into lines for the template
    tracing::trace!(
        "Expanding setup steps with params: {:?} tags: {:?}",
        params,
        steps.tags
    );
    let setup_lines: Vec<String> = steps
        .setup
        .iter()
        .map(|s| s.realize(&params, &steps.tags))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .map(|s| to_bash(&s, 1))
        .collect();

    tracing::trace!(
        "Expanding build steps with params: {:?} tags: {:?}",
        params,
        steps.tags
    );
    let build_lines: Vec<String> = steps
        .build
        .iter()
        .map(|s| s.realize(&params, &steps.tags))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .map(|s| to_bash(&s, 1))
        .collect();

    tracing::trace!(
        "Expanding capability steps with params: {:?} tags: {:?}",
        params,
        steps.tags
    );
    // Flatten all capability step lists into a single bash script block.
    // Each capability's steps are realized and concatenated in insertion order.
    let mut test_lines: Vec<String> = Vec::new();
    for cap_steps in steps.capabilities.values() {
        for s in cap_steps {
            let realized = s.realize(&params, &steps.tags)?;
            for st in realized {
                test_lines.push(to_bash(&st, 1));
            }
        }
    }

    // collect used candidate variables across all steps (on original cfg is sufficient)
    let used = collect_used_variables(&steps);

    // render template
    let vars: Vec<VarCtx> = used.iter().map(|&name| VarCtx { name }).collect();

    let ctx = TemplateCtx {
        vars,
        setup: setup_lines,
        build: build_lines,
        test: test_lines,
    };

    // 4) minijinja env + render
    let mut env = Environment::empty();
    // Load templates/… from disk
    git_driver::pull_via_cli(&mgr.config.repo_dir())?;

    env.set_loader(path_loader(
        &mgr.config.repo_dir().join("templates").join("scripts"),
    ));
    // Disable autoescaping for shell (minijinja auto-escapes only when configured;
    // with path_loader it defaults to no autoescape, so nothing else needed here.)
    let tmpl = env.get_template("steps.sh.j2")?;
    let script = tmpl.render(ctx)?;

    std::fs::write("steps.sh", script)?;
    std::fs::set_permissions(
        "steps.sh",
        std::fs::Permissions::from_mode(0o755), // -rwxr-xr-x
    )?;
    tracing::info!(
        "Bash script {} generated successfully.",
        PathBuf::from("steps.sh").canonicalize()?.display()
    );
    Ok(())
}

const CANDIDATE_VARIABLES: [&str; 12] = [
    "language",
    "workload_path",
    "workload",
    "strategy",
    "property",
    "mode",
    "inputs",
    "counterexample",
    "producer_workload_path",
    "timeout",
    "mutations",
    "experiment_id",
];

fn to_bash(s: &Step, depth: usize) -> String {
    tracing::trace!("Rendering step at depth {}: {:?}", depth, s);
    match s {
        Step::Command {
            command,
            args,
            run_at,
            mitigation,
            env,
        } => {
            let mut cmd = String::new();
            cmd.push_str(&" ".repeat(depth * 4));
            for (key, value) in env {
                cmd.push_str(&format!("{}=\"{}\" ", key, value));
            }
            cmd.push_str(command);
            for arg in args {
                cmd.push(' ');
                cmd.push_str(arg);
            }
            if let Some(run_at) = run_at {
                cmd = format!("(cd {} && {})", run_at, cmd);
            }

            if let Some(mitigation) = mitigation {
                cmd = format!(
                    "{} || echo \"command failed, please try: {}\"",
                    cmd, mitigation
                );
            }
            cmd.push('\n');
            tracing::trace!("Rendered command: {}", cmd);
            cmd
        }
        Step::Match { value, options } => {
            let mut cmd = String::new();
            let mut options = options.iter();
            let (first_option, first_step) =
                options.next().expect("Match must have at least one option");
            cmd.push_str(&format!(
                "{}if [[ ${value} == \"{first_option}\" ]]; then\n",
                " ".repeat(depth * 4)
            ));
            cmd.push_str(&to_bash(first_step, depth + 1).to_string());

            for (option, step) in options {
                // Check if option is equal to the value
                cmd.push_str(&format!(
                    "{}elif [[ ${value} == \"{option}\" ]]; then\n",
                    " ".repeat(depth * 4)
                ));
                cmd.push_str(&to_bash(step, depth + 1).to_string());
            }
            cmd.push_str(&format!(
                "{}else\n{}echo \"Unknown option: ${value}\" >&2; usage; exit 2;\n",
                " ".repeat(depth * 4),
                " ".repeat((depth + 1) * 4)
            ));
            cmd.push_str(&format!("{}fi\n", " ".repeat(depth * 4)));
            tracing::trace!("Rendered match: {}", cmd);
            cmd
        }
    }
}

