//! Shared integration-test fixture.
//!
//! `TestEtna` isolates a test from the user's real `~/.etna` by pointing
//! `ETNA_HOME` at a tempdir and staging per-test workload repos on the
//! filesystem so `git submodule add` clones them without hitting the
//! network. All tests that touch this fixture must be
//! `#[serial_test::serial]` — env vars and CWD are process-global.

#![allow(dead_code)]

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use tempfile::TempDir;

/// Path to the repo root (where `Cargo.toml` and `workloads/` live).
pub fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR points at the crate being tested.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub struct TestEtna {
    /// Holds the fake `~/.etna` tempdir; drop removes everything.
    pub home: TempDir,
    /// Per-test scratch dir used as CWD.
    pub scratch: TempDir,
}

impl TestEtna {
    pub fn new() -> Self {
        let fx = Self::new_without_etna_setup();
        // Bootstrap config.json + experiments.json.
        etna::service::config::setup(false).expect("etna setup failed");
        fx
    }

    /// Same as `new()` but stops short of running `etna::service::config::setup`.
    /// Lets CLI tests drive `etna setup` themselves against a pre-primed cache.
    pub fn new_without_etna_setup() -> Self {
        let home = TempDir::new().expect("failed to create home tempdir");
        let scratch = TempDir::new().expect("failed to create scratch tempdir");

        std::env::set_var("ETNA_HOME", home.path());
        std::env::set_var("ETNA_OFFLINE", "1");
        // Git 2.38+ refuses `file://` and absolute-path clones by default
        // (CVE-2022-39253). Re-enable for this test process — we're only
        // ever submodule-cloning fixtures we just created.
        std::env::set_var("GIT_CONFIG_COUNT", "1");
        std::env::set_var("GIT_CONFIG_KEY_0", "protocol.file.allow");
        std::env::set_var("GIT_CONFIG_VALUE_0", "always");
        std::env::set_var("GIT_AUTHOR_NAME", "etna-test");
        std::env::set_var("GIT_AUTHOR_EMAIL", "etna-test@example.com");
        std::env::set_var("GIT_COMMITTER_NAME", "etna-test");
        std::env::set_var("GIT_COMMITTER_EMAIL", "etna-test@example.com");

        std::env::set_current_dir(scratch.path()).unwrap();

        Self { home, scratch }
    }

    pub fn etna_home(&self) -> &Path {
        self.home.path()
    }

    pub fn scratch(&self) -> &Path {
        self.scratch.path()
    }

    /// Stage a Test-fixture workload as its own standalone git repo and return
    /// its absolute path — suitable to pass to `wl_svc::add_workload` as the
    /// URL. `git submodule add` clones from filesystem paths without hitting
    /// the network, so this keeps the tests offline.
    pub fn plant_workload_repo(&self, name: &str) -> PathBuf {
        let src = repo_root().join("workloads/Test").join(name);
        assert!(
            src.exists(),
            "fixture workload not found at {}",
            src.display()
        );
        let dst = self.home.path().join("workload-repos").join(name);
        copy_tree(&src, &dst);
        // Embed the shared harness inside the planted repo so steps.json's
        // `${workload_path}/harness/...` resolves once the workload is
        // submodule-cloned into an experiment.
        copy_tree(&repo_root().join("workloads/Test/harness"), &dst.join("harness"));
        git_init_commit(&dst);
        dst
    }

    /// Write a minimal `marauder.toml` registering the custom "Test" language
    /// at the given experiment root. The driver reads this via
    /// `marauders::Project::new(&experiment.path, None)` to populate
    /// `custom_languages` before doing per-workload mutations.
    pub fn plant_marauder_config(experiment_path: &Path) {
        let cfg = r#"languages = []
ignore = []
use_gitignore = false

[[custom_languages]]
name = "Test"
extension = "st"
comment_begin = "(*"
comment_end = "*)"
mutation_marker = "|"
"#;
        std::fs::write(experiment_path.join("marauder.toml"), cfg)
            .expect("failed to write marauder.toml");
    }
}

/// Recursive copy using `cp -r` — fast and preserves structure without a
/// dep on a walkdir crate. Destination parent is created first.
fn copy_tree(src: &Path, dst: &Path) {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let status = Command::new("cp")
        .arg("-r")
        .arg(src)
        .arg(dst)
        .status()
        .expect("failed to run cp");
    assert!(status.success(), "cp -r {} {} failed", src.display(), dst.display());
}

fn git_init_commit(dir: &Path) {
    run_git(dir, &["init", "--quiet"]);
    // Guard against whatever global `init.defaultBranch` is — we don't rely
    // on the name, but some git versions warn when it's unset.
    run_git(dir, &["checkout", "-q", "-b", "main"]);
    run_git(dir, &["add", "-A"]);
    run_git(
        dir,
        &[
            "-c",
            "user.email=etna-test@example.com",
            "-c",
            "user.name=etna-test",
            "commit",
            "--quiet",
            "-m",
            "initial",
            "--allow-empty",
        ],
    );
}

fn run_git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("failed to spawn git");
    assert!(
        out.status.success(),
        "git {:?} in {} failed:\nstdout: {}\nstderr: {}",
        args,
        dir.display(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
