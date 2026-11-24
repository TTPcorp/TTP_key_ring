//! File-system helpers: bounded reads and atomic writes.
//! See `docs/vault-format.md` §6.
//!
//! Only `std` is used; the `cfg(unix)` blocks are portable std extensions
//! (file mode, directory fsync), not OS-specific APIs.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Returns `path` with `suffix` appended to its file name
/// (`llavero.vault` → `llavero.vault.tmp`).
pub(crate) fn sibling(path: &Path, suffix: &str) -> Option<PathBuf> {
    let mut name: OsString = path.file_name()?.to_os_string();
    name.push(suffix);
    Some(path.with_file_name(name))
}

/// Opens a new file for writing, readable only by the owner on Unix.
fn create_private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

/// Flushes directory metadata so that a rename survives a power loss.
/// Not needed (nor possible through std) on Windows.
fn sync_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        let dir = match path.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        File::open(dir)?.sync_all()?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Reads a whole file, refusing files larger than `max_len` without
/// reading them. Returns the bytes and the file length.
pub(crate) fn read_bounded(path: &Path, max_len: u64) -> io::Result<Option<Vec<u8>>> {
    let file = File::open(path)?;
    let len = file.metadata()?.len();
    if len > max_len {
        return Ok(None);
    }
    let capacity = usize::try_from(len).map_err(|_| io::Error::from(io::ErrorKind::OutOfMemory))?;
    let mut bytes = Vec::with_capacity(capacity);
    file.take(max_len.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_len {
        return Ok(None);
    }
    Ok(Some(bytes))
}

/// Writes `bytes` to a temp file and fsyncs it.
fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = create_private(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Copies `from` to `to` atomically (via a temp file) and durably.
fn copy_atomic(from: &Path, to: &Path, tmp: &Path) -> io::Result<()> {
    let result = (|| {
        let mut src = File::open(from)?;
        let mut dst = create_private(tmp)?;
        io::copy(&mut src, &mut dst)?;
        dst.sync_all()?;
        drop(dst);
        fs::rename(tmp, to)
    })();
    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }
    result
}

/// Atomically replaces `path` with `bytes`:
///
/// 1. write and fsync `path.tmp`;
/// 2. if `path` exists and `keep_backup`, copy it to `path.bak`
///    (itself atomically, through `path.bak.tmp`);
/// 3. rename `path.tmp` over `path` (atomic on NTFS and POSIX);
/// 4. fsync the directory (Unix).
///
/// On failure before step 3 the original file is untouched and the temp
/// files are removed.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8], keep_backup: bool) -> io::Result<()> {
    let invalid = || io::Error::from(io::ErrorKind::InvalidInput);
    let tmp = sibling(path, ".tmp").ok_or_else(invalid)?;
    let bak = sibling(path, ".bak").ok_or_else(invalid)?;
    let bak_tmp = sibling(path, ".bak.tmp").ok_or_else(invalid)?;

    let result = (|| {
        write_synced(&tmp, bytes)?;
        if keep_backup && path.exists() {
            copy_atomic(path, &bak, &bak_tmp)?;
        }
        fs::rename(&tmp, path)?;
        sync_dir(path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_appends_suffix() {
        let p = Path::new("dir").join("llavero.vault");
        assert_eq!(
            sibling(&p, ".tmp").unwrap(),
            Path::new("dir").join("llavero.vault.tmp")
        );
        assert!(sibling(Path::new(".."), ".tmp").is_none());
    }

    #[test]
    fn write_atomic_creates_replaces_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.vault");

        write_atomic(&path, b"one", true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"one");
        assert!(!dir.path().join("v.vault.bak").exists());

        write_atomic(&path, b"two", true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(fs::read(dir.path().join("v.vault.bak")).unwrap(), b"one");

        write_atomic(&path, b"three", false).unwrap();
        assert_eq!(fs::read(dir.path().join("v.vault.bak")).unwrap(), b"one");

        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 2, "no temp files left: {names:?}");
    }

    #[test]
    fn failed_write_leaves_original_intact() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.vault");
        write_atomic(&path, b"original", true).unwrap();
        // A directory where the temp file should go makes step 1 fail.
        fs::create_dir(dir.path().join("v.vault.tmp")).unwrap();
        assert!(write_atomic(&path, b"new", true).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
    }

    #[cfg(unix)]
    #[test]
    fn files_are_private_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.vault");
        write_atomic(&path, b"x", true).unwrap();
        write_atomic(&path, b"y", true).unwrap();
        for name in ["v.vault", "v.vault.bak"] {
            let mode = fs::metadata(dir.path().join(name))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "{name}");
        }
    }

    #[test]
    fn read_bounded_rejects_large_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        fs::write(&path, [0u8; 10]).unwrap();
        assert_eq!(read_bounded(&path, 10).unwrap().unwrap().len(), 10);
        assert!(read_bounded(&path, 9).unwrap().is_none());
    }
}
