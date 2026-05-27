//! Recoverable-deletion staging area under `$ETNA_HOME/trash/`.
//!
//! Callers send directories here instead of `fs::remove_dir_all` when they
//! want the deletion to be reversible. Each call first sweeps any entry in
//! the trash whose mtime is older than [`RETENTION`]; the sweep is what
//! eventually frees disk — nothing else does.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{config::EtnaConfig, error_context::Context};

/// How long trashed entries are kept before the next sweep removes them.
pub const RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// `$ETNA_HOME/trash/` — created on demand by [`move_to_trash`].
pub fn trash_dir() -> anyhow::Result<PathBuf> {
    Ok(EtnaConfig::get_etna_dir()?.join("trash"))
}

/// Move `src` into the trash, first sweeping any entries past [`RETENTION`].
///
/// Returns the final path inside the trash. The destination name is
/// `<basename>-<unix_ts>` so repeated deletions of the same directory don't
/// collide. Falls back to copy + remove if a plain rename fails (e.g. the
/// trash lives on a different filesystem than the source).
pub fn move_to_trash(src: &Path) -> anyhow::Result<PathBuf> {
    let trash = trash_dir()?;
    fs::create_dir_all(&trash)
        .with_context(|| format!("Failed to create trash dir at '{}'", trash.display()))?;

    // Sweep first, so disk pressure from the new arrival can reclaim
    // expired entries without waiting for a future deletion.
    if let Err(e) = sweep(RETENTION) {
        tracing::warn!("trash sweep failed (continuing): {e:#}");
    }

    let basename = src.file_name().ok_or_else(|| {
        anyhow::anyhow!("Cannot trash a path without a file name: {}", src.display())
    })?;
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut dest = trash.join(format!("{}-{ts}", basename.to_string_lossy()));
    // Defensive: if two deletes land in the same second, suffix with a counter.
    let mut suffix = 1;
    while dest.exists() {
        dest = trash.join(format!("{}-{ts}-{suffix}", basename.to_string_lossy()));
        suffix += 1;
    }

    match fs::rename(src, &dest) {
        Ok(()) => {}
        Err(_) => {
            // Likely cross-device. Fall back to recursive copy + remove.
            copy_dir_recursive(src, &dest).with_context(|| {
                format!(
                    "Failed to copy '{}' into trash at '{}'",
                    src.display(),
                    dest.display()
                )
            })?;
            fs::remove_dir_all(src).with_context(|| {
                format!(
                    "Failed to remove '{}' after copying into trash",
                    src.display()
                )
            })?;
        }
    }

    tracing::info!(
        "Moved '{}' to trash at '{}' (retention: {} days)",
        src.display(),
        dest.display(),
        RETENTION.as_secs() / 86400
    );

    Ok(dest)
}

/// Permanently remove every direct child of the trash dir whose mtime is
/// older than `max_age`. Individual failures are logged and skipped — one
/// broken entry shouldn't block the rest of the sweep.
pub fn sweep(max_age: Duration) -> anyhow::Result<()> {
    let trash = trash_dir()?;
    let entries = match fs::read_dir(&trash) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(anyhow::anyhow!(
                "Failed to read trash at '{}': {e}",
                trash.display()
            ))
        }
    };

    let now = SystemTime::now();
    let mut removed = 0usize;

    for entry in entries.flatten() {
        let path = entry.path();
        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("trash sweep: failed to stat '{}': {e}", path.display());
                continue;
            }
        };
        let mtime = metadata.modified().unwrap_or(now);
        let age = now.duration_since(mtime).unwrap_or(Duration::ZERO);
        if age < max_age {
            continue;
        }
        let result = if metadata.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match result {
            Ok(()) => {
                removed += 1;
                tracing::debug!("trash sweep: removed '{}'", path.display());
            }
            Err(e) => tracing::warn!("trash sweep: failed to remove '{}': {e}", path.display()),
        }
    }

    if removed > 0 {
        tracing::info!(
            "trash sweep: removed {removed} expired entr{}",
            if removed == 1 { "y" } else { "ies" }
        );
    }

    Ok(())
}

fn copy_dir_recursive(src: &Path, dest: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let child_src = entry.path();
        let child_dest = dest.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&child_src, &child_dest)?;
        } else if file_type.is_symlink() {
            #[cfg(unix)]
            {
                let target = fs::read_link(&child_src)?;
                std::os::unix::fs::symlink(target, &child_dest)?;
            }
            #[cfg(not(unix))]
            {
                fs::copy(&child_src, &child_dest)?;
            }
        } else {
            fs::copy(&child_src, &child_dest)?;
        }
    }
    Ok(())
}
