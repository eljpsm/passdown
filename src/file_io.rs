//! Atomic in-place file replacement with identity and hard-link safety
//! checks. Fixes are written to a temporary file beside the target and
//! persisted by rename, so a crash or failed write never leaves a
//! half-written document.

use std::fs::{File, OpenOptions, Permissions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use tempfile::Builder;

/// A physical file's identity: device + inode on Unix, volume serial number +
/// file index on Windows. Used to deduplicate path aliases during discovery
/// and to detect the target being swapped out between read and write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FileIdentity {
    first: u64,
    second: u64,
}

/// A snapshot of the metadata `atomic_replace` needs to commit safely.
#[derive(Debug)]
pub(crate) struct InspectedFile {
    pub identity: FileIdentity,
    pub link_count: u64,
    pub permissions: Permissions,
}

#[derive(Debug)]
pub(crate) enum CommitError {
    Io(io::Error),
    /// The path resolved to a different physical file than the one inspected
    /// at discovery time. Writing would clobber something we never read.
    IdentityChanged,
    /// The target has multiple hard links. Replacing it by rename would
    /// silently detach the other names, so it is rejected instead.
    HardLinked(u64),
}

impl std::fmt::Display for CommitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommitError::Io(err) => err.fmt(formatter),
            CommitError::IdentityChanged => {
                formatter.write_str("file identity changed during formatting")
            }
            CommitError::HardLinked(count) => write!(formatter, "target has {count} links"),
        }
    }
}

impl From<io::Error> for CommitError {
    fn from(err: io::Error) -> Self {
        CommitError::Io(err)
    }
}

/// Resolve symlinks to the real target and capture its identity. Fixes then
/// rewrite the linked-to file, preserving the symlink. Commits use the
/// identity to detect a swapped-out target.
pub(crate) fn canonicalize_and_inspect(path: &Path) -> io::Result<(PathBuf, InspectedFile)> {
    let target_path = std::fs::canonicalize(path)?;
    let file = File::open(&target_path)?;
    Ok((target_path, inspect_file(file)?))
}

/// Replace `path`'s contents atomically: write to a temporary file in the
/// same directory, copy the target's permissions onto it, and rename it over
/// the target. Fails without touching the target if the file's identity no
/// longer matches `expected_identity` or if it has other hard links.
pub(crate) fn atomic_replace(
    path: &Path,
    expected_identity: FileIdentity,
    contents: &[u8],
) -> Result<(), CommitError> {
    atomic_replace_with(path, expected_identity, |file| file.write_all(contents))
}

fn atomic_replace_with(
    path: &Path,
    expected_identity: FileIdentity,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> Result<(), CommitError> {
    // Atomic replacement can bypass a read-only target when its directory is
    // writable. Preserve existing authorization behavior by requiring write
    // access to the target first.
    let target = OpenOptions::new().write(true).open(path)?;
    let inspected = inspect_file(target)?;
    if inspected.identity != expected_identity {
        return Err(CommitError::IdentityChanged);
    }
    if inspected.link_count > 1 {
        return Err(CommitError::HardLinked(inspected.link_count));
    }

    let parent = path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} has no parent directory", path.display()),
        )
    })?;
    let mut temporary = Builder::new()
        .prefix(".passdown-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    write(temporary.as_file_mut())?;
    temporary.as_file().set_permissions(inspected.permissions)?;
    temporary
        .persist(path)
        .map_err(|err| CommitError::Io(err.error))?;
    Ok(())
}

#[cfg(unix)]
fn inspect_file(file: File) -> io::Result<InspectedFile> {
    use std::os::unix::fs::MetadataExt;

    let metadata = file.metadata()?;
    Ok(InspectedFile {
        identity: FileIdentity {
            first: metadata.dev(),
            second: metadata.ino(),
        },
        link_count: metadata.nlink(),
        permissions: metadata.permissions(),
    })
}

#[cfg(windows)]
fn inspect_file(file: File) -> io::Result<InspectedFile> {
    let handle = winapi_util::Handle::from_file(file);
    let metadata = handle.as_file().metadata()?;
    let information = winapi_util::file::information(&handle)?;
    Ok(InspectedFile {
        identity: FileIdentity {
            first: information.volume_serial_number(),
            second: information.file_index(),
        },
        link_count: information.number_of_links(),
        permissions: metadata.permissions(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_write_leaves_original_untouched_and_cleans_up() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("document.md");
        std::fs::write(&path, "original\n").unwrap();
        let (_, inspected) = canonicalize_and_inspect(&path).unwrap();

        let result = atomic_replace_with(&path, inspected.identity, |file| {
            file.write_all(b"partial")?;
            Err(io::Error::other("injected failure"))
        });

        assert!(matches!(result, Err(CommitError::Io(_))));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
        let names: Vec<_> = std::fs::read_dir(directory.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, ["document.md"]);
    }

    #[test]
    fn identity_change_leaves_replacement_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("original.md");
        let replacement = directory.path().join("replacement.md");
        std::fs::write(&original, "original\n").unwrap();
        std::fs::write(&replacement, "replacement\n").unwrap();
        let (_, inspected) = canonicalize_and_inspect(&original).unwrap();

        let result = atomic_replace(&replacement, inspected.identity, b"formatted\n");

        assert!(matches!(result, Err(CommitError::IdentityChanged)));
        assert_eq!(
            std::fs::read_to_string(&replacement).unwrap(),
            "replacement\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn replacement_preserves_unix_mode() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("document.md");
        std::fs::write(&path, "before\n").unwrap();
        std::fs::set_permissions(&path, Permissions::from_mode(0o640)).unwrap();
        let (_, inspected) = canonicalize_and_inspect(&path).unwrap();

        atomic_replace(&path, inspected.identity, b"after\n").unwrap();

        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o640);
    }
}
