use std::path::{Path, PathBuf};

use tracing::debug;

use crate::error_context::Context;

const ETNA_COMMITTER_NAME: &str = "ETNA Commit Bot";
const ETNA_COMMITTER_EMAIL: &str = "etna-bot@users.noreply.github.com";

pub(crate) fn initialize_git_repo(path: &PathBuf, msg: &str) -> anyhow::Result<()> {
    let repo_path = path.display().to_string();
    // Initialize a git repository
    let git_repo = git2::Repository::init(path)
        .with_context(|| format!("Failed to initialize git repository at '{repo_path}'"))?;
    let mut index = git_repo
        .index()
        .with_context(|| format!("Failed to get index for repo '{repo_path}'"))?;
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .with_context(|| format!("Failed to add files to index for repo '{repo_path}'"))?;
    index
        .write()
        .with_context(|| format!("Failed to write index for repo '{repo_path}'"))?;
    let tree_id = index
        .write_tree()
        .with_context(|| format!("Failed to write tree for repo '{repo_path}'"))?;
    let tree = git_repo
        .find_tree(tree_id)
        .with_context(|| format!("Failed to find tree for repo '{repo_path}'"))?;

    if let Ok(head) = git_repo.head() {
        if let Ok(head_commit) = head.peel_to_commit() {
            if head_commit.tree_id() == tree_id {
                debug!("No changes detected; skipping commit");
                return Ok(());
            }
        }
    }

    let signature = git2::Signature::now(ETNA_COMMITTER_NAME, ETNA_COMMITTER_EMAIL)
        .with_context(|| format!("Failed to create signature for repo '{repo_path}'"))?;
    git_repo
        .commit(Some("HEAD"), &signature, &signature, msg, &tree, &[])
        .with_context(|| format!("Failed to commit in repo '{repo_path}'"))?;
    Ok(())
}

/// Commit the entire repo with the given message.
pub(crate) fn commit(repo_path: &Path, message: &str) -> anyhow::Result<String> {
    debug!("repo path: {}", repo_path.display());
    let repo_display = repo_path.display().to_string();
    let git_repo = git2::Repository::open_ext(
        repo_path,
        git2::RepositoryOpenFlags::NO_SEARCH,
        std::iter::empty::<&std::ffi::OsStr>(),
    )
    .with_context(|| {
        format!(
            "Failed to open git repository at '{}' without parent search",
            repo_display
        )
    })?;

    let mut index = git_repo
        .index()
        .with_context(|| format!("Failed to get index for repo '{repo_display}'"))?;
    // Don't clear the index: existing submodule gitlinks (mode 160000) are
    // tracked there from the parent commit and have no representation as
    // ordinary files on disk, so a clear-and-readd loop drops them. Instead
    // we layer add_all (new + modified files) and update_all (deletions of
    // previously-tracked files) on top of the existing index, which matches
    // `git add -A` semantics and preserves gitlinks.

    let mut last_seen_path: Option<String> = None;
    let mut last_skipped_nested_repo: Option<String> = None;
    let repo_root = repo_path.to_path_buf();
    index
        .add_all(
            ["*"],
            git2::IndexAddOption::DEFAULT,
            Some(&mut |path, _| {
                let p = path.display().to_string();
                last_seen_path = Some(p.clone());
                // Embedded repositories appear as directory-style matches (e.g. "foo/").
                // Skip only those nested repo roots; keep normal directory traversal behavior.
                if p.ends_with('/') {
                    let nested_root = repo_root.join(path);
                    if nested_root.join(".git").exists() {
                        last_skipped_nested_repo = Some(p);
                        return 1; // Skip this matched path and continue.
                    }
                }
                0
            }),
        )
        .with_context(|| match last_seen_path {
            Some(ref p) => match last_skipped_nested_repo {
                Some(ref skipped) => format!(
                    "Failed to add files to index for repo '{repo_display}' (last path: '{p}', skipped nested repo: '{skipped}')"
                ),
                None => {
                    format!("Failed to add files to index for repo '{repo_display}' (last path: '{p}')")
                }
            },
            None => format!("Failed to add files to index for repo '{repo_display}'"),
        })?;

    // Reflect deletions of previously-tracked files (e.g. a manual `rm` or
    // `git rm`) so the auto-commit doesn't keep stale entries. update_all
    // only touches paths already in the index, so it can't re-introduce
    // submodule contents or other files that were intentionally absent.
    index
        .update_all(["*"], None)
        .with_context(|| format!("Failed to update index for repo '{repo_display}'"))?;

    index
        .write()
        .with_context(|| format!("Failed to write index for repo '{repo_display}'"))?;
    debug!(
        "index {:?}",
        index
            .iter()
            .map(|entry| std::ffi::CString::new(&entry.path[..]).unwrap())
            .collect::<Vec<_>>()
    );

    let tree_id = index
        .write_tree()
        .with_context(|| format!("Failed to write tree for repo '{repo_display}'"))?;
    let tree = git_repo
        .find_tree(tree_id)
        .with_context(|| format!("Failed to find tree for repo '{repo_display}'"))?;

    let signature = git2::Signature::now(ETNA_COMMITTER_NAME, ETNA_COMMITTER_EMAIL)
        .with_context(|| format!("Failed to create signature for repo '{repo_display}'"))?;

    git_repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            &format!("automated commit: '{message}'",),
            &tree,
            &[&git_repo
                .head()
                .with_context(|| format!("Failed to get head for repo '{repo_display}'"))?
                .peel_to_commit()
                .with_context(|| {
                    format!("Failed to peel head to commit for repo '{repo_display}'")
                })?],
        )
        .with_context(|| format!("Failed to commit in repo '{repo_display}'"))?;

    let head = git_repo
        .head()
        .with_context(|| format!("Failed to get head for repo '{repo_display}'"))?;
    let head = head
        .peel_to_commit()
        .with_context(|| format!("Failed to peel head to commit for repo '{repo_display}'"))?;
    Ok(head.id().to_string())
}

/// Get the hash of the head of a git repository
pub(crate) fn _head_hash(repo_path: &Path) -> anyhow::Result<String> {
    let git_repo = git2::Repository::open(repo_path).context("Failed to open git repository")?;
    let head = git_repo.head().context("Failed to get head")?;
    let head = head.peel_to_commit().context("Failed to peel to commit")?;
    Ok(head.id().to_string())
}

/// `ETNA_OFFLINE=1` means "don't hit the network." Local paths (`file://`,
/// absolute filesystem paths) don't, so they're allowed through.
fn is_local_git_url(url: &str) -> bool {
    url.starts_with("file://") || Path::new(url).is_absolute()
}

/// Shell out to `git clone [--branch <ref>] <url> <dest>`. Refuses when
/// `ETNA_OFFLINE` is set and the URL is not a local path.
pub(crate) fn git_clone(url: &str, reference: Option<&str>, dest: &Path) -> anyhow::Result<()> {
    if std::env::var_os("ETNA_OFFLINE").is_some() && !is_local_git_url(url) {
        anyhow::bail!("Cannot clone '{}' while ETNA_OFFLINE is set", url);
    }
    let mut cmd = std::process::Command::new("git");
    cmd.arg("clone");
    if let Some(r) = reference {
        cmd.arg("--branch").arg(r);
    }
    cmd.arg(url).arg(dest);
    let status = cmd
        .status()
        .with_context(|| format!("Failed to invoke 'git clone {}'", url))?;
    if !status.success() {
        anyhow::bail!("git clone {} failed with status {}", url, status);
    }
    Ok(())
}

/// Like `git_clone` but passes `--recurse-submodules` so submodule'd workloads
/// come down with the experiment.
pub(crate) fn git_clone_recursive(
    url: &str,
    reference: Option<&str>,
    dest: &Path,
) -> anyhow::Result<()> {
    if std::env::var_os("ETNA_OFFLINE").is_some() && !is_local_git_url(url) {
        anyhow::bail!("Cannot clone '{}' while ETNA_OFFLINE is set", url);
    }
    let mut cmd = std::process::Command::new("git");
    cmd.arg("clone").arg("--recurse-submodules");
    if let Some(r) = reference {
        cmd.arg("--branch").arg(r);
    }
    cmd.arg(url).arg(dest);
    let status = cmd
        .status()
        .with_context(|| format!("Failed to invoke 'git clone --recurse-submodules {}'", url))?;
    if !status.success() {
        anyhow::bail!(
            "git clone --recurse-submodules {} failed with status {}",
            url,
            status
        );
    }
    Ok(())
}

/// Run `git -C <repo> submodule add [--branch <ref>] <url> <path_in_repo>`,
/// then initialise any nested submodules inside the freshly added repo so a
/// workload that itself depends on submodules (e.g. a shared support lib)
/// arrives with its full source tree populated. The caller is responsible for
/// committing the resulting staged changes (`.gitmodules` + the new submodule
/// gitlink).
pub(crate) fn git_submodule_add(
    repo: &Path,
    url: &str,
    reference: Option<&str>,
    path_in_repo: &Path,
) -> anyhow::Result<()> {
    if std::env::var_os("ETNA_OFFLINE").is_some() && !is_local_git_url(url) {
        anyhow::bail!("Cannot add submodule '{}' while ETNA_OFFLINE is set", url);
    }
    let mut cmd = std::process::Command::new("git");
    cmd.arg("-C").arg(repo).arg("submodule").arg("add");
    if let Some(r) = reference {
        cmd.arg("--branch").arg(r);
    }
    cmd.arg(url).arg(path_in_repo);
    let status = cmd.status().with_context(|| {
        format!(
            "Failed to invoke 'git -C {} submodule add {}'",
            repo.display(),
            url
        )
    })?;
    if !status.success() {
        anyhow::bail!(
            "git submodule add {} in {} failed with status {}",
            url,
            repo.display(),
            status
        );
    }

    // `git submodule add` does not recurse, so nested submodules end up as
    // empty gitlinks. Fix that up in a second pass scoped to the new path.
    let added_path = repo.join(path_in_repo);
    let nested_status = std::process::Command::new("git")
        .arg("-C")
        .arg(&added_path)
        .arg("submodule")
        .arg("update")
        .arg("--init")
        .arg("--recursive")
        .status()
        .with_context(|| {
            format!(
                "Failed to invoke 'git -C {} submodule update --init --recursive'",
                added_path.display()
            )
        })?;
    if !nested_status.success() {
        anyhow::bail!(
            "git submodule update --init --recursive in {} failed with status {}",
            added_path.display(),
            nested_status
        );
    }
    Ok(())
}

/// Run `git -C <repo> submodule deinit -f <path>` followed by `git -C <repo>
/// rm -f <path>` so a removed workload leaves the outer repo in a clean,
/// committable state (both `.gitmodules` update and the gitlink removal are
/// staged).
pub(crate) fn git_submodule_remove(repo: &Path, path_in_repo: &Path) -> anyhow::Result<()> {
    let deinit = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("submodule")
        .arg("deinit")
        .arg("-f")
        .arg(path_in_repo)
        .status()
        .with_context(|| {
            format!(
                "Failed to invoke 'git -C {} submodule deinit {}'",
                repo.display(),
                path_in_repo.display()
            )
        })?;
    if !deinit.success() {
        anyhow::bail!(
            "git submodule deinit {} in {} failed with status {}",
            path_in_repo.display(),
            repo.display(),
            deinit
        );
    }
    let rm = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("rm")
        .arg("-f")
        .arg(path_in_repo)
        .status()
        .with_context(|| {
            format!(
                "Failed to invoke 'git -C {} rm {}'",
                repo.display(),
                path_in_repo.display()
            )
        })?;
    if !rm.success() {
        anyhow::bail!(
            "git rm {} in {} failed with status {}",
            path_in_repo.display(),
            repo.display(),
            rm
        );
    }
    Ok(())
}
