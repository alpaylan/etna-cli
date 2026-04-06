use std::{collections::HashMap, fmt::Display, hash::Hash, path::PathBuf};

use itertools::Itertools as _;
use serde::{Deserialize, Serialize};

use crate::{property::Property, strategy::Strategy};
use crate::error_context::Context as _;
use marauders::Variation;

/// Represents a command that can be executed in the context of a workload.
pub(crate) struct Command {
    /// The command to be executed.
    pub(crate) command: String,
    /// The arguments to the command.
    pub(crate) args: Vec<String>,
    /// The directory where the command should be run.
    pub(crate) run_at: Option<String>,
    /// Optional mitigation information for the command.
    /// This can be used to specify how to handle potential issues or failures.
    #[allow(dead_code)]
    pub(crate) mitigation: Option<String>,
    /// Environment variables to set when running the command.
    pub(crate) env: HashMap<String, String>,
}

impl From<&Command> for std::process::Command {
    fn from(cmd: &Command) -> Self {
        let mut command = std::process::Command::new(&cmd.command);
        command.args(&cmd.args).envs(&cmd.env);
        if let Some(run_at) = &cmd.run_at {
            command.current_dir(run_at);
        }
        command
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(run_at) = &self.run_at {
            write!(f, "cd {} && ", run_at)?;
        }
        write!(
            f,
            "{} {} {}",
            self.env
                .iter()
                .map(|(k, v)| format!("\"{}\"=\"{}\"", k, v))
                .collect::<Vec<_>>()
                .join(" "),
            self.command,
            self.args.join(" ")
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Represents a step in a workload, which can either be a command to run or a match condition
/// that decides which command to run based on parameters and tags.
pub enum Step {
    #[serde()]
    Command {
        command: String,
        #[serde(skip_serializing_if = "Vec::is_empty", default)]
        args: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        run_at: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        mitigation: Option<String>,
        #[serde(skip_serializing_if = "HashMap::is_empty", default)]
        env: HashMap<String, String>,
    },
    Match {
        value: String,
        options: HashMap<String, Step>,
    },
}

impl Step {
    pub(crate) fn decide(
        &self,
        params: &HashMap<String, String>,
        tags: &HashMap<String, Vec<String>>,
    ) -> Command {
        tracing::debug!("deciding step: {self} with params: {params:?} and tags: {tags:?}");
        match self {
            Step::Command {
                command,
                args,
                run_at,
                mitigation,
                env,
            } => Command {
                command: command.clone(),
                args: args.clone(),
                run_at: run_at.clone(),
                mitigation: mitigation.clone(),
                env: env.clone(),
            },
            Step::Match { value, options } => {
                let guard = params.get(value).unwrap();
                tracing::debug!("obtaining guard '{guard}' for tags_ {tags:?}");

                if let Some(step) = options.get(guard) {
                    return step.decide(params, tags);
                }
                println!("Guard '{guard}' not found in options {options:?}, trying tags");
                println!("Available tags: {tags:?}");
                let tags_ = tags
                    .iter()
                    .filter_map(|(k, v)| if v.contains(guard) { Some(k) } else { None })
                    .collect::<Vec<_>>();
                // let tags_ = tags.get(guard).unwrap();
                for (k, step) in options {
                    if tags_.contains(&k) {
                        return step.decide(params, tags);
                    }
                }

                panic!("None of the options fit")
            }
        }
    }

    pub(crate) fn contains(&self, k: &str) -> bool {
        let result = match self {
            Step::Command { command, args, .. } => {
                command.contains(k) || args.iter().any(|a| a.contains(k))
            }
            Step::Match { options, .. } => options.values().any(|s| s.contains(k)),
        };
        if result {
            tracing::trace!("step '{self}' contains key '{k}'");
        } else {
            tracing::trace!("step '{self}' does not contain key '{k}'");
        }
        result
    }

    pub(crate) fn replace(&mut self, s1: &str, s2: &str) {
        let original_step = self.clone();
        match self {
            Step::Command {
                command,
                args,
                run_at,
                env,
                mitigation: _,
            } => {
                *command = command.replace(s1, s2);
                *args = args.iter().map(|arg| arg.replace(s1, s2)).collect();
                *run_at = run_at.as_ref().map(|r| r.replace(s1, s2));
                *env = env
                    .iter()
                    .map(|(k, v)| (k.replace(s1, s2), v.replace(s1, s2)))
                    .collect();
            }
            Step::Match { options, .. } => {
                for step in options.values_mut() {
                    step.replace(s1, s2)
                }
            }
        }
        tracing::debug!("replaced step: '{}' with '{}'", original_step, self);
    }

    pub(crate) fn realize(
        &self,
        params: &HashMap<String, String>,
        tags: &HashMap<String, Vec<String>>,
    ) -> anyhow::Result<Vec<Step>> {
        let step = self.clone();
        let mut steps = vec![];
        // elaboration step
        let mut elaborates = vec![];

        // Find all parameters that need elaboration
        for (param, _) in params.iter().sorted_by(|a, b| b.0.len().cmp(&a.0.len())) {
            if step.contains(&format!("!{{{}}}", param)) {
                elaborates.push(param);
            }
        }
        for (tag, _) in tags.iter() {
            if step.contains(&format!("!{{{}}}", tag)) {
                elaborates.push(tag);
            }
        }
        let all_elaborations = elaborates
            .iter()
            .map(|key| {
                tags.get(*key)
                    .unwrap_or_else(|| panic!("missing tag {}", key))
                    .clone()
            })
            .multi_cartesian_product()
            .collect::<Vec<Vec<_>>>();

        for elaboration_set in all_elaborations {
            let mut step = step.clone();
            for (i, val) in elaboration_set.iter().enumerate() {
                step.replace(&format!("!{{{}}}", elaborates[i]), val);
            }
            steps.push(step);
        }

        for step in steps.iter_mut() {
            let params = params.iter().sorted_by(|a, b| b.0.len().cmp(&a.0.len()));

            for (key, value) in params {
                step.replace(&format!("${{{}}}", key), value);
            }
        }

        Ok(steps)
    }
}

impl Display for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Step::Command { command, args, .. } => write!(f, "{} {}", command, args.join(" ")),
            Step::Match { value, options } => {
                write!(f, "match '{}': ", value)?;
                for (k, step) in options {
                    write!(f, "\n\t({} => {})", k, step)?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub(crate) struct Steps {
    #[serde(rename = "setup_steps")]
    pub(crate) setup: Vec<Step>,
    #[serde(rename = "build_steps")]
    pub(crate) build: Vec<Step>,
    #[serde(rename = "test_steps")]
    pub(crate) test: Vec<Step>,
    #[serde(default)]
    pub(crate) tags: HashMap<String, Vec<String>>,
}

impl Steps {
    pub(crate) fn get_steps(json: &serde_json::Value, step_index: &str) -> Option<Vec<Step>> {
        let step = json.get(step_index);

        if let Some(step) = step {
            let steps = serde_json::from_value::<Vec<Step>>(step.clone());
            if let Ok(steps) = steps {
                return Some(steps);
            } else {
                tracing::debug!("Step: {}", step);
                tracing::debug!("Error: {}", steps.unwrap_err());
                tracing::error!("Failed to parse step: '{}'", step_index);
            }
        }
        None
    }

    pub(crate) fn with_default(json: &serde_json::Value, default: &Steps) -> Self {
        let setup = Self::get_steps(json, "setup_steps").unwrap_or(default.setup.clone());
        let build = Self::get_steps(json, "build_steps").unwrap_or(default.build.clone());
        let test = Self::get_steps(json, "test_steps").unwrap_or(default.test.clone());

        let tags = if let Some(tags) = json.get("tags") {
            serde_json::from_value(tags.clone()).unwrap_or_else(|_| default.tags.clone())
        } else {
            default.tags.clone()
        };

        Self {
            setup,
            build,
            test,
            tags,
        }
    }

    pub(crate) fn from_value(json: &serde_json::Value) -> anyhow::Result<Self> {
        let setup = Self::get_steps(json, "setup_steps").context("could not find setup_steps")?;
        let build = Self::get_steps(json, "build_steps").context("could not find build_steps")?;
        let test = Self::get_steps(json, "test_steps").context("could not find test_steps")?;

        let tags = if let Some(tags) = json.get("tags") {
            serde_json::from_value(tags.clone()).context("could not parse tags")?
        } else {
            HashMap::new()
        };

        Ok(Self {
            setup,
            build,
            test,
            tags,
        })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Hash, Eq)]
pub struct WorkloadMetadata {
    pub name: String,
    pub language: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Workload {
    pub name: String,
    pub language: String,
    pub dir: PathBuf,
    pub properties: Vec<Property>,
    pub variations: Vec<Variation>,
    pub strategies: Vec<Strategy>,
    pub(crate) steps: Steps,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Language {
    pub name: String,
    pub(crate) steps: Steps,
}
