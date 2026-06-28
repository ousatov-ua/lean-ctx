//! Shared low-level file-edit I/O primitives (epic #1008).
//!
//! Extracted from `ctx_edit` so the anchored editor (`ctx_patch`) reuses the
//! *exact same* TOCTOU-safe read→verify→atomic-write path instead of growing a
//! second, drifting copy of this security-critical code:
//!
//! * symlink rejection (`reject_symlink` + `O_NOFOLLOW`),
//! * read-size cap + UTF-8 validation,
//! * whole-file preimage fingerprint (size + mtime + BLAKE3) for the TOCTOU
//!   guard ([`ensure_preimage_still_matches`]),
//! * permission-preserving crash-atomic `rename`, with the read-only-directory
//!   in-place fallback (#459),
//! * the read-only-roots write choke point (#475).
//!
//! One implementation = one audited boundary. `ctx_edit` (`str_replace`) and
//! `ctx_patch` (anchored) both apply through these functions, so a fix here
//! protects both tools at once.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Size + mtime + content hash of a file, used as a TOCTOU fingerprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileFingerprint {
    pub(crate) size: u64,
    pub(crate) mtime_ms: u64,
    pub(crate) md5: String,
}

/// A file read for editing: its fingerprint, permissions, raw bytes, decoded
/// text and whether it uses CRLF line endings.
#[derive(Clone, Debug)]
pub(crate) struct FilePreimage {
    pub(crate) fp: FileFingerprint,
    pub(crate) permissions: std::fs::Permissions,
    pub(crate) bytes: Vec<u8>,
    pub(crate) text: String,
    pub(crate) uses_crlf: bool,
}

pub(crate) fn system_time_to_millis(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Rejects symlinks at `path` (TOCTOU protection, same boundary as
/// `core::io_boundary::read_file_nofollow`): a symlink planted inside the jail
/// after the jail check could otherwise read or overwrite files outside it.
pub(crate) fn reject_symlink(path: &Path) -> Result<(), String> {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        // Windows: also covers NTFS junctions/reparse points (GL#442).
        if crate::core::pathutil::is_symlink_or_reparse(&meta) {
            return Err(format!(
                "ERROR: {} is a symlink — refusing to edit through it (TOCTOU protection). \
                 Edit the symlink target directly via its real path.",
                path.display()
            ));
        }
    }
    Ok(())
}

pub(crate) fn read_file_bytes_limited(
    path: &Path,
    cap: usize,
) -> Result<(Vec<u8>, std::fs::Metadata), String> {
    reject_symlink(path)?;

    if let Ok(meta) = std::fs::metadata(path)
        && meta.len() > cap as u64
    {
        return Err(format!(
            "ERROR: file too large ({} bytes, cap {} via LCTX_MAX_READ_BYTES): {}",
            meta.len(),
            cap,
            path.display()
        ));
    }

    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        // Defense in depth alongside `reject_symlink`: O_NOFOLLOW closes the
        // race between the lstat check and the open.
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = opts.open(path).map_err(|e| {
        #[cfg(unix)]
        if e.raw_os_error() == Some(libc::ELOOP) {
            return format!(
                "ERROR: {} is a symlink — refusing to edit through it (TOCTOU protection).",
                path.display()
            );
        }
        format!("ERROR: cannot open {}: {e}", path.display())
    })?;

    use std::io::Read;
    let mut raw: Vec<u8> = Vec::new();
    let mut limited = (&mut file).take((cap as u64).saturating_add(1));
    limited
        .read_to_end(&mut raw)
        .map_err(|e| format!("ERROR: cannot read {}: {e}", path.display()))?;
    if raw.len() > cap {
        return Err(format!(
            "ERROR: file too large (cap {} via LCTX_MAX_READ_BYTES): {}",
            cap,
            path.display()
        ));
    }

    let meta = file
        .metadata()
        .map_err(|e| format!("ERROR: cannot stat {}: {e}", path.display()))?;
    Ok((raw, meta))
}

pub(crate) fn fingerprint_from_bytes(bytes: &[u8], meta: &std::fs::Metadata) -> FileFingerprint {
    FileFingerprint {
        size: bytes.len() as u64,
        mtime_ms: meta.modified().map_or(0, system_time_to_millis),
        md5: crate::core::hasher::hash_hex(bytes),
    }
}

pub(crate) fn read_preimage(
    path: &Path,
    cap: usize,
    allow_lossy_utf8: bool,
) -> Result<FilePreimage, String> {
    let (bytes, meta) = read_file_bytes_limited(path, cap)?;
    let permissions = meta.permissions();
    let fp = fingerprint_from_bytes(&bytes, &meta);

    let text = if allow_lossy_utf8 {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        String::from_utf8(bytes.clone()).map_err(|_| {
            format!(
                "ERROR: file is not valid UTF-8 (binary/encoding). Refusing to edit: {}",
                path.display()
            )
        })?
    };
    let uses_crlf = text.contains("\r\n");

    Ok(FilePreimage {
        fp,
        permissions,
        bytes,
        text,
        uses_crlf,
    })
}

/// Re-reads the file and confirms its fingerprint still equals `expected`
/// (TOCTOU guard): a concurrent writer between the preimage read and the write
/// is detected here so the edit aborts instead of clobbering newer bytes.
pub(crate) fn ensure_preimage_still_matches(
    path: &Path,
    expected: &FileFingerprint,
    cap: usize,
) -> Result<(), String> {
    let (bytes, meta) = read_file_bytes_limited(path, cap)?;
    let now = fingerprint_from_bytes(&bytes, &meta);
    if &now != expected {
        return Err(format!(
            "ERROR: file changed since read (TOCTOU guard). Re-read and retry: {}\nexpected: size={}, mtime_ms={}, md5={}\nactual:   size={}, mtime_ms={}, md5={}",
            path.display(),
            expected.size,
            expected.mtime_ms,
            expected.md5,
            now.size,
            now.mtime_ms,
            now.md5
        ));
    }
    Ok(())
}

pub(crate) fn default_backup_path(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let filename = path.file_name()?.to_string_lossy();
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    Some(parent.join(format!("{filename}.lean-ctx.bak.{pid}.{nanos}")))
}

pub(crate) fn write_atomic_bytes_with_permissions(
    path: &Path,
    bytes: &[u8],
    permissions: Option<&std::fs::Permissions>,
) -> Result<(), String> {
    // Read-only-roots choke point (#475). Every edit write — replace, create,
    // and the pre-edit backup — funnels here, including a backup whose raw
    // `backup_path` bypasses the dispatch jail, so this single guard makes the
    // whole tool default-deny inside a read-only root before any byte is written
    // or temp/dir created.
    crate::core::pathjail::enforce_writable(path)?;

    // The rename below would *replace* a symlink at `path` (safe), but the edit
    // pipeline read through this path moments ago — a symlink here means the
    // read/write pair straddles two different files. Reject for consistency
    // with the read-side O_NOFOLLOW boundary.
    reject_symlink(path)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    match try_atomic_write(path, bytes, permissions) {
        Ok(()) => Ok(()),
        // Read-only *directory* holding a writable *file* inode — common in
        // sandboxes/containers that bind-mount individual files rw on top of a
        // read-only directory subvolume (e.g. ~/.config/opencode ro,
        // opencode.jsonc rw). The same-dir tempfile + rename dance needs
        // directory write permission, which the ro mount denies, but the
        // existing inode is writable, so overwrite it in place. Not
        // crash-atomic, yet the preimage guard (size/mtime/hash) already gates
        // the write, so a torn write is caught on the next read instead of
        // being silently accepted. (GH #459)
        Err(e) if is_readonly_dir_error(&e) && path.is_file() => {
            in_place_overwrite(path, bytes, permissions).map_err(|fallback_err| {
                format!(
                    "ERROR: atomic write failed ({e}); in-place fallback also failed: {fallback_err} ({})",
                    path.display()
                )
            })
        }
        Err(e) => Err(format!("ERROR: atomic write failed: {e} ({})", path.display())),
    }
}

/// Durable, crash-atomic write: a temp file in the **same directory** followed by
/// `rename` over the target. Requires write permission on the *parent
/// directory*; the caller handles the read-only-directory fallback.
fn try_atomic_write(
    path: &Path,
    bytes: &[u8],
    permissions: Option<&std::fs::Permissions>,
) -> std::io::Result<()> {
    use std::io::Write;

    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid path (no parent directory)",
        )
    })?;
    let filename = path
        .file_name()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid path (no filename)",
            )
        })?
        .to_string_lossy();

    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let tmp = parent.join(format!(".{filename}.lean-ctx.tmp.{pid}.{nanos}"));

    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        let _ = f.flush();
        let _ = f.sync_all();
    }

    if let Some(perms) = permissions {
        let _ = std::fs::set_permissions(&tmp, perms.clone());
    }

    #[cfg(windows)]
    {
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
    }

    if let Err(e) = std::fs::rename(&tmp, path) {
        // Don't leave a half-written temp behind before the caller decides
        // whether to fall back.
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// In-place overwrite of an existing file inode (`O_WRONLY|O_TRUNC`). Works when
/// the parent directory is read-only but the file itself is writable. Not
/// crash-atomic — used only as a fallback when the atomic path is impossible.
pub(crate) fn in_place_overwrite(
    path: &Path,
    bytes: &[u8],
    permissions: Option<&std::fs::Permissions>,
) -> std::io::Result<()> {
    use std::io::Write;

    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NOFOLLOW: a symlink swapped in after reject_symlink must never be
        // followed (mirrors the read-side O_NOFOLLOW boundary).
        opts.custom_flags(libc::O_NOFOLLOW);
    }

    let mut f = opts.open(path)?;
    f.write_all(bytes)?;
    let _ = f.flush();
    let _ = f.sync_all();

    if let Some(perms) = permissions {
        let _ = std::fs::set_permissions(path, perms.clone());
    }
    Ok(())
}

/// True for errors that mean "this directory won't accept create/rename" even
/// though the target file may be writable: `EROFS` (read-only fs) plus
/// `EACCES`/`EPERM` (directory write denied).
pub(crate) fn is_readonly_dir_error(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        return true;
    }
    #[cfg(unix)]
    {
        matches!(
            e.raw_os_error(),
            Some(libc::EROFS | libc::EACCES | libc::EPERM)
        )
    }
    #[cfg(not(unix))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readonly_dir_error_classification() {
        assert!(is_readonly_dir_error(&std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        )));
        assert!(!is_readonly_dir_error(&std::io::Error::from(
            std::io::ErrorKind::NotFound
        )));
        #[cfg(unix)]
        {
            assert!(is_readonly_dir_error(&std::io::Error::from_raw_os_error(
                libc::EROFS
            )));
            assert!(is_readonly_dir_error(&std::io::Error::from_raw_os_error(
                libc::EACCES
            )));
            assert!(is_readonly_dir_error(&std::io::Error::from_raw_os_error(
                libc::EPERM
            )));
        }
    }

    #[cfg(unix)]
    #[test]
    fn in_place_overwrite_truncates_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.jsonc");
        std::fs::write(&path, b"longer original content").unwrap();
        in_place_overwrite(&path, b"short", None).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"short");
    }

    #[test]
    fn detects_toctou_via_preimage_guard() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("toctou.txt");
        std::fs::write(&path, "aaa\n").unwrap();
        let cap = crate::core::limits::max_read_bytes();
        let pre = read_preimage(&path, cap, false).unwrap();
        std::fs::write(&path, "bbb\n").unwrap();
        let err = ensure_preimage_still_matches(&path, &pre.fp, cap).unwrap_err();
        assert!(err.contains("TOCTOU guard"), "unexpected error: {err}");
    }

    #[test]
    fn read_preimage_rejects_invalid_utf8_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bin.dat");
        std::fs::write(&path, [0xff, 0xfe, 0xfd]).unwrap();
        let cap = crate::core::limits::max_read_bytes();
        let err = read_preimage(&path, cap, false).unwrap_err();
        assert!(err.contains("not valid UTF-8"), "got: {err}");
        // Lossy mode tolerates it.
        assert!(read_preimage(&path, cap, true).is_ok());
    }

    #[test]
    fn fingerprint_is_content_addressed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fp.txt");
        std::fs::write(&path, "hello\n").unwrap();
        let cap = crate::core::limits::max_read_bytes();
        let a = read_preimage(&path, cap, false).unwrap().fp;
        let b = read_preimage(&path, cap, false).unwrap().fp;
        assert_eq!(a, b, "same bytes → same fingerprint");
        assert_eq!(a.md5, crate::core::hasher::hash_hex(b"hello\n"));
    }
}
