use std::path::{Path, PathBuf};

use anyhow::Context;
use log::debug;

pub(crate) fn initialize_git_repo(path: &PathBuf, msg: &str) -> anyhow::Result<()> {
    // Initialize a git repository
    let git_repo = git2::Repository::init(path).context("Failed to initialize git repository")?;
    let mut index = git_repo.index().context("Failed to get index")?;
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .context("Failed to add files to index")?;
    index.write().context("Failed to write index")?;
    let tree_id = index.write_tree().context("Failed to write tree")?;
    let tree = git_repo.find_tree(tree_id).context("Failed to find tree")?;

    let signature = git2::Signature::now("Alperen Keles", "akeles@umd.edu")
        .context("Failed to create signature")?;
    git_repo
        .commit(Some("HEAD"), &signature, &signature, msg, &tree, &[])
        .context("Failed to commit")?;
    Ok(())
}

pub(crate) fn commit_add_workload(language: &str, workload: &str) -> anyhow::Result<()> {
    // Add contents of 'workloads/<language>/<workload>' directory to the git repository
    let git_repo = git2::Repository::open_from_env().context("Failed to open git repository")?;
    let mut index = git_repo.index().context("Failed to get index")?;

    // Add the workload to the index
    index
        .add_all(
            [PathBuf::from("workloads")
                .join(language)
                .join(workload)
                .as_path()],
            git2::IndexAddOption::DEFAULT,
            None,
        )
        .context("Failed to add workload to index")?;
    // Add the config file to the index
    index
        .add_path(PathBuf::from("config.toml").as_path())
        .context("Failed to add workload to index")?;
    // Commit the changes
    let mut index = git_repo.index().context("Failed to get index")?;
    index.write().context("Failed to write index")?;
    let tree_id = index.write_tree().context("Failed to write tree")?;
    let tree = git_repo.find_tree(tree_id).context("Failed to find tree")?;

    let signature = git2::Signature::now("Alperen Keles", "akeles@umd.edu")
        .context("Failed to create signature")?;

    git_repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            &format!("automated commit: add workload '{}/{}'", language, workload),
            &tree,
            &[&git_repo
                .head()
                .context("Failed to get head")?
                .peel_to_commit()
                .context("Failed to peel to commit")?],
        )
        .context("Failed to commit")?;

    Ok(())
}

pub(crate) fn commit_remove_workload(language: &str, workload: &str) -> anyhow::Result<()> {
    // Add contents of 'workloads/<language>/<workload>' directory to the git repository
    let git_repo = git2::Repository::open_from_env().context("Failed to open git repository")?;

    // Commit the changes
    let mut index = git_repo.index().context("Failed to get index")?;

    // Add the workload to the index
    index
        .add_all(
            [PathBuf::from("workloads")
                .join(language)
                .join(workload)
                .as_path()],
            git2::IndexAddOption::DEFAULT,
            None,
        )
        .context("Failed to add workload to index")?;

    // Add the config file to the index
    index
        .add_path(PathBuf::from("config.toml").as_path())
        .context("Failed to add workload to index")?;

    index.write().context("Failed to write index")?;
    let tree_id = index.write_tree().context("Failed to write tree")?;
    let tree = git_repo.find_tree(tree_id).context("Failed to find tree")?;

    let signature = git2::Signature::now("Alperen Keles", "akeles@umd.edu")
        .context("Failed to create signature")?;

    git_repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            &format!(
                "automated commit: remove workload '{}/{}'",
                language, workload
            ),
            &tree,
            &[&git_repo
                .head()
                .context("Failed to get head")?
                .peel_to_commit()
                .context("Failed to peel to commit")?],
        )
        .context("Failed to commit")?;

    Ok(())
}

pub(crate) fn clone_etna(path: &PathBuf) -> anyhow::Result<()> {
    // Clone the etna repository
    git2::Repository::clone("https://github.com/jwshii/etna.git", path)
        .context("Failed to clone ETNA repository")?;

    Ok(())
}

pub(crate) fn change_branch(repo_path: &PathBuf, branch: &str) -> anyhow::Result<()> {
    // Change the branch of the etna repository
    let git_repo = git2::Repository::open(repo_path).context("Failed to open git repository")?;
    let mut remote = git_repo
        .find_remote("origin")
        .context("Failed to find remote")?;
    remote
        .fetch(&[branch], None, None)
        .context("Failed to fetch remote")?;

    debug!(
        "list of branches: {:?}",
        git_repo
            .branches(None)
            .unwrap()
            .map(|branch| branch.unwrap().0.name().unwrap().unwrap().to_string())
            .collect::<Vec<_>>()
    );

    let origin_branch = format!("origin/{}", branch);
    let branch = git_repo
        .find_branch(&origin_branch, git2::BranchType::Remote)
        .context("Failed to find branch")?;
    let branch = branch.into_reference();
    let branch = branch
        .peel_to_commit()
        .context("Failed to peel to commit")?;
    let branch = branch.into_object();

    git_repo
        .reset(&branch, git2::ResetType::Hard, None)
        .context("Failed to reset branch")?;

    Ok(())
}

/// Get the hash of a path in a git repository
pub(crate) fn hash(repo_path: &Path, index_path: &Path) -> anyhow::Result<String> {
    debug!("repo path: {}", repo_path.display());
    let git_repo = git2::Repository::open(repo_path).context("Failed to open git repository")?;

    debug!("index path: {}", index_path.display());
    let mut index = git_repo.index().context("Failed to get index")?;
    index.clear().context("Failed to clear index")?;

    index
        .add_all([index_path], git2::IndexAddOption::DEFAULT, None)
        .context("Failed to add files to index")?;

    debug!(
        "index {:?}",
        index
            .iter()
            .map(|entry| std::ffi::CString::new(&entry.path[..]).unwrap())
            .collect::<Vec<_>>()
    );

    let tree_id = index.write_tree().context("Failed to write tree")?;
    let tree = git_repo.find_tree(tree_id).context("Failed to find tree")?;

    debug!("tree id: {}", tree.id());

    Ok(tree.id().to_string())
}

/// Get the hash of the head of a git repository
pub(crate) fn head_hash(repo_path: &Path) -> anyhow::Result<String> {
    let git_repo = git2::Repository::open(repo_path).context("Failed to open git repository")?;
    let head = git_repo.head().context("Failed to get head")?;
    let head = head.peel_to_commit().context("Failed to peel to commit")?;
    Ok(head.id().to_string())
}
