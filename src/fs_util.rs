use std::io::Write as _;
use std::path::Path;

use crate::error_context::Context as _;

/// Serialize `value` as pretty JSON and replace `path` atomically.
///
/// Writes to a temporary file in the same directory, then renames it over the
/// destination. A crash or serialization error leaves the previous contents
/// intact instead of truncating `path` to an empty or half-written file, which
/// is what `File::create` + `to_writer` would do.
pub(crate) fn write_json_atomically<T: serde::Serialize>(
    path: &Path,
    value: &T,
) -> anyhow::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent).with_context(|| {
        format!(
            "Failed to create a temporary file next to '{}'",
            path.display()
        )
    })?;
    serde_json::to_writer_pretty(&mut tmp, value)
        .with_context(|| format!("Failed to serialize JSON for '{}'", path.display()))?;
    tmp.flush()
        .with_context(|| format!("Failed to flush temporary file for '{}'", path.display()))?;
    tmp.persist(path)
        .with_context(|| format!("Failed to replace '{}'", path.display()))?;
    Ok(())
}
