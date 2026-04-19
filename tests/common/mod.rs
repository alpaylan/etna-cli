//! Shared integration-test fixture.
//!
//! `TestEtna` isolates a test from the user's real `~/.etna` by pointing
//! `ETNA_HOME` at a tempdir, copying the repo's `workloads/` and `docs/`
//! into the fake cache, and pre-`git init`-ing it so `etna` skips the
//! `git clone` + `git pull` network paths. All tests that touch this
//! fixture must be `#[serial_test::serial]` — env vars and CWD are
//! process-global.

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
        std::env::set_var("GIT_AUTHOR_NAME", "etna-test");
        std::env::set_var("GIT_AUTHOR_EMAIL", "etna-test@example.com");
        std::env::set_var("GIT_COMMITTER_NAME", "etna-test");
        std::env::set_var("GIT_COMMITTER_EMAIL", "etna-test@example.com");

        let cache_dir = home.path().join(".etna_cache");
        std::fs::create_dir_all(&cache_dir).unwrap();

        let root = repo_root();
        // Only the Test fixture is needed for integration tests. Copying all
        // workload languages (Rust/Rocq/Haskell/…) takes several seconds per
        // test, so scope this down.
        std::fs::create_dir_all(cache_dir.join("workloads")).unwrap();
        copy_tree(&root.join("workloads/Test"), &cache_dir.join("workloads/Test"));
        std::fs::create_dir_all(cache_dir.join("docs/workloads")).unwrap();
        for name in ["t1.json", "t2.json"] {
            std::fs::copy(
                root.join("docs/workloads").join(name),
                cache_dir.join("docs/workloads").join(name),
            )
            .expect("failed to copy docs fixture");
        }
        // `etna bash` loads templates/scripts/steps.sh.j2 from the cache.
        copy_tree(&root.join("templates"), &cache_dir.join("templates"));

        // Give the fake cache a committed .git so `setup()` skips `git clone`.
        git_init_commit(&cache_dir);

        std::env::set_current_dir(scratch.path()).unwrap();

        Self { home, scratch }
    }

    pub fn etna_home(&self) -> &Path {
        self.home.path()
    }

    pub fn scratch(&self) -> &Path {
        self.scratch.path()
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
