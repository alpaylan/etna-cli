use std::{collections::HashMap, fmt::Display, hash::Hash, path::PathBuf};

use itertools::Itertools as _;
use serde::{Deserialize, Serialize};

use crate::error_context::Context as _;
use crate::{property::Property, strategy::Strategy};
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
        tracing::trace!("deciding step: {self} with params: {params:?} and tags: {tags:?}");
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
                tracing::trace!("obtaining guard '{guard}' for tags_ {tags:?}");

                if let Some(step) = options.get(guard) {
                    return step.decide(params, tags);
                }
                tracing::info!("Guard '{guard}' not found in options {options:?}, trying tags");
                tracing::info!("Available tags: {tags:?}");
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
        tracing::trace!("replaced step: '{}' with '{}'", original_step, self);
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

/// Capabilities a workload can expose. Each capability is a typed pipeline stage:
/// - `Solve`:  full PBT campaign. Output: campaign-result JSON on stdout/stderr.
/// - `Sample`: produce inputs (with optional per-input metadata). Output: input-stream JSON on stdout.
/// - `Test`:   consume a list of inputs, run a property over them. Output: campaign-result JSON.
/// - `Shrink`: consume a single failing input, return a minimized one. Output: campaign-result JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Solve,
    Sample,
    Test,
    Shrink,
}

impl Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Capability::Solve => "solve",
            Capability::Sample => "sample",
            Capability::Test => "test",
            Capability::Shrink => "shrink",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub(crate) struct Steps {
    #[serde(rename = "setup_steps")]
    pub(crate) setup: Vec<Step>,
    #[serde(rename = "build_steps")]
    pub(crate) build: Vec<Step>,
    #[serde(default)]
    pub(crate) capabilities: HashMap<Capability, Vec<Step>>,
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

    fn get_capabilities(
        json: &serde_json::Value,
    ) -> Option<HashMap<Capability, Vec<Step>>> {
        let caps = json.get("capabilities")?;
        match serde_json::from_value::<HashMap<Capability, Vec<Step>>>(caps.clone()) {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::error!("Failed to parse 'capabilities': {}", e);
                None
            }
        }
    }

    pub(crate) fn from_value(json: &serde_json::Value) -> anyhow::Result<Self> {
        let setup = Self::get_steps(json, "setup_steps").context("could not find setup_steps")?;
        let build = Self::get_steps(json, "build_steps").context("could not find build_steps")?;
        let capabilities = Self::get_capabilities(json).unwrap_or_default();

        let tags = if let Some(tags) = json.get("tags") {
            serde_json::from_value(tags.clone()).context("could not parse tags")?
        } else {
            HashMap::new()
        };

        Ok(Self {
            setup,
            build,
            capabilities,
            tags,
        })
    }

    pub(crate) fn capability(&self, cap: Capability) -> anyhow::Result<&Vec<Step>> {
        self.capabilities.get(&cap).ok_or_else(|| {
            anyhow::anyhow!(
                "workload does not declare capability '{}' in steps.json",
                cap
            )
        })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Hash, Eq)]
pub struct WorkloadMetadata {
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Workload {
    pub name: String,
    /// Source language. Read from the workload's own `etna.toml`. Used for
    /// marauders file-extension resolution and for tagging metric rows; not
    /// part of the workload's public identity.
    pub language: String,
    pub dir: PathBuf,
    pub properties: Vec<Property>,
    pub variations: Vec<Variation>,
    pub strategies: Vec<Strategy>,
    pub(crate) steps: Steps,
}

/// Deserialized form of `etna.toml` at a workload repo root.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct WorkloadManifest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Required: used by marauders to pick the file-extension glob.
    pub language: String,
    /// Source crate/package name (e.g. Cargo crate, PyPI package).
    /// Serialized as `crate` in `etna.toml`.
    #[serde(default, rename = "crate", skip_serializing_if = "Option::is_none")]
    pub crate_name: Option<String>,
    /// Base commit SHA on which every `etna/<variant>` branch is built
    /// (base + one marauder diff). Used by `etna workload check` to verify
    /// that every variant branch descends from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<ManifestTaskGroup>,
    /// PBT strategy names this workload's runner accepts. Required —
    /// `etna workload add` seeds one task per (property × strategy) pair, so
    /// a missing or empty list produces no runnable tests. Each name must
    /// match an exact identifier the workload binary's dispatcher expects
    /// (e.g. cedar-lean: "plausible"/"etna"; haskell-bst: "Quick"/"Hedgehog";
    /// rocq-bst: "BespokeGenerator"/"TypeBasedGenerator").
    pub strategies: Vec<String>,
    /// Upstream fix commits that were considered for injection but rejected.
    /// Rendered in `BUGS.md` under "Dropped Candidates" for audit trail.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped: Vec<DroppedCandidate>,
}

/// One `[[tasks]]` block: a set of mutations paired with the properties to
/// evaluate against them. Converted at workload-add time into a `Test` entry
/// in the experiment's `tests/<name>.json`.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ManifestTaskGroup {
    pub mutations: Vec<String>,
    pub tasks: Vec<ManifestTask>,
    /// Upstream provenance: where the bug came from (PR, issue, commit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceContext>,
    /// How the bug is introduced into the workload (marauders vs. patch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub injection: Option<InjectionSpec>,
    /// Human-readable bug narrative (invariant, trigger mechanism).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bug: Option<BugDetails>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ManifestTask {
    pub property: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub witnesses: Vec<Witness>,
}

/// A known failing input. `Input` is a literal serialised value in the
/// workload's language; `TestFn` names a test function in the workload's
/// test suite (used by the `-etna` crate forks). Each variant carries an
/// optional `note` for per-witness commentary rendered in `BUGS.md`.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum Witness {
    Input {
        input: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    TestFn {
        test_fn: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
}

/// Upstream provenance for a task group. `summary` is required whenever
/// a `[source]` block is present — it is the one piece of prose that survives
/// when the upstream PR body drifts.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct SourceContext {
    /// Canonical upstream repo URL (e.g. "https://github.com/Lokathor/tinyvec").
    pub repo: String,
    /// Fix commit SHAs in chronological order. Vec to handle multi-commit fixes;
    /// length-1 is the common case.
    pub commits: Vec<String>,
    /// Commit subject lines aligned positionally with `commits`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commit_subjects: Vec<String>,
    /// PR numbers associated with the fix.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prs: Vec<u32>,
    /// Issue numbers associated with the fix.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<u32>,
    /// URL of an associated discussion / GitHub thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discussion: Option<String>,
    /// Free-text origin for bugs that surfaced outside PR/issue flow
    /// (fuzzer finding, internal report, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    /// 1-3 line human excerpt of the fix rationale. Truth-of-record when
    /// upstream PR body drifts.
    pub summary: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct InjectionSpec {
    pub kind: InjectionKind,
    /// Source files touched by the injection.
    pub files: Vec<String>,
    /// Specific locations (file + line + symbol). Vec to support multi-file bugs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<FileLoc>,
    /// Path (relative to workload root) to the patch file, when `kind = Patch`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum InjectionKind {
    /// Bug lives in a marauders `[[variant]]` source-level mutation.
    Marauders,
    /// Bug lives in a `.patch` file applied atop `base_commit`.
    Patch,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct FileLoc {
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    /// Enclosing symbol (function / method), e.g. "ArrayVec::swap_remove".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct BugDetails {
    /// Short human name for the bug, e.g. "debug_alternate_empty".
    pub short_name: String,
    /// One paragraph describing the invariant that the bug violates.
    pub invariant: String,
    /// One paragraph describing how the mutation triggers the bug.
    pub how_triggered: String,
}

/// An upstream fix commit that was considered but not injected as a variant.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct DroppedCandidate {
    pub commit: String,
    /// One-line rationale, rendered verbatim in `BUGS.md`.
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

impl WorkloadManifest {
    /// Read `<dir>/etna.toml` and parse it.
    pub fn read(dir: &std::path::Path) -> anyhow::Result<Self> {
        let manifest_path = dir.join("etna.toml");
        let body = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("Failed to read '{}'", manifest_path.display()))?;
        toml::from_str::<Self>(&body)
            .with_context(|| format!("Failed to parse '{}'", manifest_path.display()))
    }
}

#[cfg(test)]
mod manifest_tests {
    use super::*;

    #[test]
    fn legacy_minimal_manifest_still_parses() {
        // Fields added in the v2 extension must all be optional except
        // `strategies`, which became required in 0.1.12.
        let toml = r#"
            name = "example"
            language = "rust"
            strategies = ["s1"]
        "#;
        let m: WorkloadManifest = toml::from_str(toml).expect("minimal parse");
        assert_eq!(m.name, "example");
        assert_eq!(m.language, "rust");
        assert_eq!(m.strategies, vec!["s1"]);
        assert!(m.tasks.is_empty());
        assert!(m.dropped.is_empty());
        assert!(m.crate_name.is_none());
        assert!(m.base_commit.is_none());
    }

    #[test]
    fn manifest_missing_strategies_is_rejected() {
        // `strategies` is required as of 0.1.12 — surface a clear error so
        // stale manifests fail loudly at `etna workload add` time.
        let toml = r#"
            name = "example"
            language = "rust"
        "#;
        let err = toml::from_str::<WorkloadManifest>(toml).expect_err("must reject");
        assert!(
            format!("{err}").contains("missing field `strategies`"),
            "expected missing-strategies error, got: {err}"
        );
    }

    #[test]
    fn legacy_tasks_only_manifest_still_parses() {
        // Mirrors T1/T2 test-fixture shape: `tasks` without any of the new
        // source/injection/bug blocks.
        let toml = r#"
            name = "example"
            language = "rust"
            strategies = ["s1"]

            [[tasks]]
            mutations = ["foo_1234567_1"]
            tasks = [{ property = "P1", witnesses = [{ test_fn = "case_a" }] }]
        "#;
        let m: WorkloadManifest = toml::from_str(toml).expect("tasks parse");
        assert_eq!(m.tasks.len(), 1);
        assert_eq!(m.tasks[0].mutations, vec!["foo_1234567_1"]);
        assert_eq!(m.tasks[0].tasks.len(), 1);
        assert_eq!(m.tasks[0].tasks[0].property, "P1");
        assert!(m.tasks[0].source.is_none());
        assert!(m.tasks[0].injection.is_none());
        assert!(m.tasks[0].bug.is_none());
        match &m.tasks[0].tasks[0].witnesses[0] {
            Witness::TestFn { test_fn, note } => {
                assert_eq!(test_fn, "case_a");
                assert!(note.is_none());
            }
            _ => panic!("expected TestFn witness"),
        }
    }

    #[test]
    fn extended_manifest_roundtrips() {
        let toml = r#"
            name = "tinyvec"
            language = "rust"
            crate = "tinyvec"
            base_commit = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
            strategies = ["proptest"]

            [[tasks]]
            mutations = ["debug_alternate_empty_a711c72_1"]

            [tasks.source]
            repo = "https://github.com/Lokathor/tinyvec"
            commits = ["a711c72abcdef1234567890abcdef1234567890a"]
            commit_subjects = ["Fix Debug impl for empty ArrayVec"]
            prs = [123]
            issues = [120]
            summary = "The Debug impl emitted a stray comma for empty ArrayVec."

            [tasks.injection]
            kind = "marauders"
            files = ["src/arrayvec.rs"]
            locations = [{ file = "src/arrayvec.rs", line = 42, symbol = "ArrayVec::fmt" }]

            [tasks.bug]
            short_name = "debug_alternate_empty"
            invariant = "Debug formatting must not emit a trailing comma on empty containers."
            how_triggered = "The loop unconditionally wrote a comma after the first element."

            [[tasks.tasks]]
            property = "ArrayvecDebugMatchesSlice"
            witnesses = [
                { test_fn = "case_empty", note = "case_empty exposes the stray comma" },
                { input = "ArrayVec::<[u8; 4]>::new()" },
            ]

            [[dropped]]
            commit = "cafebabe1234567890abcdef1234567890abcdef"
            reason = "Fixed a whitespace-only lint; not a real invariant bug."
            subject = "style: rustfmt pass"
        "#;
        let m: WorkloadManifest = toml::from_str(toml).expect("extended parse");
        // Round-trip: serialize + reparse, compare for structural equality.
        let s = toml::to_string(&m).expect("serialize");
        let m2: WorkloadManifest = toml::from_str(&s).expect("reparse");
        assert_eq!(m, m2);

        assert_eq!(m.crate_name.as_deref(), Some("tinyvec"));
        assert_eq!(
            m.base_commit.as_deref(),
            Some("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
        );
        assert_eq!(m.dropped.len(), 1);
        assert_eq!(m.dropped[0].reason, "Fixed a whitespace-only lint; not a real invariant bug.");

        let g = &m.tasks[0];
        let src = g.source.as_ref().expect("source");
        assert_eq!(src.repo, "https://github.com/Lokathor/tinyvec");
        assert_eq!(src.prs, vec![123]);
        assert_eq!(src.issues, vec![120]);
        assert!(src.origin.is_none());

        let inj = g.injection.as_ref().expect("injection");
        assert!(matches!(inj.kind, InjectionKind::Marauders));
        assert_eq!(inj.files, vec!["src/arrayvec.rs"]);
        assert_eq!(inj.locations[0].line, Some(42));
        assert_eq!(inj.locations[0].symbol.as_deref(), Some("ArrayVec::fmt"));

        let bug = g.bug.as_ref().expect("bug");
        assert_eq!(bug.short_name, "debug_alternate_empty");

        let witnesses = &g.tasks[0].witnesses;
        assert_eq!(witnesses.len(), 2);
        match &witnesses[0] {
            Witness::TestFn { test_fn, note } => {
                assert_eq!(test_fn, "case_empty");
                assert_eq!(note.as_deref(), Some("case_empty exposes the stray comma"));
            }
            _ => panic!("expected TestFn witness"),
        }
        match &witnesses[1] {
            Witness::Input { input, note } => {
                assert_eq!(input, "ArrayVec::<[u8; 4]>::new()");
                assert!(note.is_none());
            }
            _ => panic!("expected Input witness"),
        }
    }

    #[test]
    fn injection_kind_patch_parses() {
        let toml = r#"
            name = "aho-corasick"
            language = "rust"
            strategies = ["proptest"]

            [[tasks]]
            mutations = ["ac_patched_0000000_1"]

            [tasks.injection]
            kind = "patch"
            files = ["src/lib.rs"]
            patch = "patches/ac_patched.patch"

            [[tasks.tasks]]
            property = "Matches"
        "#;
        let m: WorkloadManifest = toml::from_str(toml).expect("patch parse");
        let inj = m.tasks[0].injection.as_ref().unwrap();
        assert!(matches!(inj.kind, InjectionKind::Patch));
        assert_eq!(inj.patch.as_deref(), Some("patches/ac_patched.patch"));
    }
}
