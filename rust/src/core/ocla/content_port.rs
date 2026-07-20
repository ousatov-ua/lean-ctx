//! CompressionContentPort — resolves content refs to bounded byte slices and
//! persists compressed results as content-addressed BLAKE3 refs.
//!
//! Security invariants:
//! - All reads go through PathJail (no traversal outside project root)
//! - Unix reads use descriptor-relative openat with O_NOFOLLOW per component
//! - Reads are bounded to MAX_CONTENT_BYTES (prevents OOM on large files)
//! - Persisted refs use BLAKE3 content-addressing (collision-resistant)

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::core::ocla::OclaError;
use crate::core::ocla::types::OclaResult;
use crate::core::pathjail;

const MAX_CONTENT_BYTES: usize = 512 * 1024;
const MAX_CACHE_ENTRIES: usize = 256;

pub struct CompressionContentPort {
    project_root: PathBuf,
    cache: Mutex<ContentCache>,
}

struct ContentCache {
    entries: Vec<CacheEntry>,
}

struct CacheEntry {
    ref_key: String,
    data: Vec<u8>,
}

impl Default for ContentCache {
    fn default() -> Self {
        Self {
            entries: Vec::with_capacity(64),
        }
    }
}

impl CompressionContentPort {
    pub fn new(project_root: impl Into<PathBuf>) -> Option<Self> {
        let project_root = project_root.into();
        let metadata = fs::symlink_metadata(&project_root).ok()?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return None;
        }
        let project_root = project_root.canonicalize().ok()?;
        let metadata = fs::symlink_metadata(&project_root).ok()?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return None;
        }
        Some(Self {
            project_root,
            cache: Mutex::new(ContentCache::default()),
        })
    }

    /// Resolve a `file:<relative_path>` ref to bounded bytes.
    /// Rejects symlinks at any component, path traversal, and oversized files.
    pub fn resolve(&self, content_ref: &str) -> OclaResult<Vec<u8>> {
        let rel_path = content_ref
            .strip_prefix("file:")
            .ok_or_else(|| OclaError::InvalidRequest("content_ref must use file: scheme".into()))?;

        // Validate containment via PathJail (traversal safety)
        let _jailed = pathjail::jail_path(Path::new(rel_path), &self.project_root)
            .map_err(|e| OclaError::InvalidRequest(format!("path jail: {e}")))?;

        let file = open_content_file(&self.project_root, rel_path)
            .map_err(|e| OclaError::InvalidRequest(format!("open: {e}")))?;
        let meta = file
            .metadata()
            .map_err(|e| OclaError::InvalidRequest(format!("metadata: {e}")))?;

        if !meta.file_type().is_file() {
            return Err(OclaError::InvalidRequest("not a regular file".into()));
        }

        if meta.len() > MAX_CONTENT_BYTES as u64 {
            return Err(OclaError::InvalidRequest(format!(
                "file exceeds {MAX_CONTENT_BYTES} byte limit"
            )));
        }

        let mut data = Vec::with_capacity(meta.len() as usize);
        file.take((MAX_CONTENT_BYTES + 1) as u64)
            .read_to_end(&mut data)
            .map_err(|e| OclaError::InvalidRequest(format!("read: {e}")))?;

        if data.len() > MAX_CONTENT_BYTES {
            return Err(OclaError::InvalidRequest(format!(
                "file exceeds {MAX_CONTENT_BYTES} byte limit"
            )));
        }

        Ok(data)
    }

    /// Persist compressed bytes and return a `blake3:<hex>` content-addressed ref.
    pub fn persist(&self, data: &[u8]) -> OclaResult<String> {
        if data.len() > MAX_CONTENT_BYTES {
            return Err(OclaError::InvalidRequest(
                "compressed output exceeds size limit".into(),
            ));
        }

        let hash = blake3::hash(data);
        let ref_key = format!("blake3:{}", hash.to_hex());

        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if cache.entries.iter().any(|e| e.ref_key == ref_key) {
            return Ok(ref_key);
        }

        if cache.entries.len() >= MAX_CACHE_ENTRIES {
            let quarter = cache.entries.len() / 4;
            cache.entries.drain(..quarter);
        }

        cache.entries.push(CacheEntry {
            ref_key: ref_key.clone(),
            data: data.to_vec(),
        });

        Ok(ref_key)
    }

    /// Retrieve previously persisted content by its BLAKE3 ref.
    pub fn retrieve(&self, ref_key: &str) -> OclaResult<Vec<u8>> {
        let cache = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        cache
            .entries
            .iter()
            .find(|e| e.ref_key == ref_key)
            .map(|e| e.data.clone())
            .ok_or_else(|| OclaError::InvalidRequest(format!("ref not found: {ref_key}")))
    }
}

#[cfg(unix)]
fn open_content_file(root: &Path, relative: &str) -> io::Result<fs::File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::ffi::OsStrExt;

    fn normalize_open_error(error: io::Error) -> io::Error {
        if error.raw_os_error() == Some(libc::ELOOP) {
            io::Error::other("symlink detected")
        } else {
            error
        }
    }

    fn open_directory(parent: libc::c_int, name: &std::ffi::OsStr) -> io::Result<OwnedFd> {
        let name = CString::new(name.as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))?;
        let fd = unsafe {
            libc::openat(
                parent,
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            Err(normalize_open_error(io::Error::last_os_error()))
        } else {
            Ok(unsafe { OwnedFd::from_raw_fd(fd) })
        }
    }

    fn open_root(root: &Path) -> io::Result<OwnedFd> {
        let root_name = CString::new("/").expect("literal has no NUL");
        let fd = unsafe {
            libc::open(
                root_name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            return Err(normalize_open_error(io::Error::last_os_error()));
        }
        let mut current = unsafe { OwnedFd::from_raw_fd(fd) };
        for component in root.components() {
            if let std::path::Component::Normal(name) = component {
                current = open_directory(current.as_raw_fd(), name)?;
            }
        }
        Ok(current)
    }

    let mut components = Vec::new();
    for component in Path::new(relative).components() {
        match component {
            std::path::Component::Normal(name) => components.push(name),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid relative path",
                ));
            }
        }
    }
    let (last, parents) = components
        .split_last()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty relative path"))?;
    let mut parent = open_root(root)?;
    for name in parents {
        parent = open_directory(parent.as_raw_fd(), name)?;
    }
    let name = CString::new(last.as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "NUL in path"))?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if fd < 0 {
        return Err(normalize_open_error(io::Error::last_os_error()));
    }
    Ok(fs::File::from(unsafe { OwnedFd::from_raw_fd(fd) }))
}

#[cfg(not(unix))]
fn open_content_file(root: &Path, relative: &str) -> io::Result<fs::File> {
    let mut current = root.to_path_buf();
    for component in Path::new(relative).components() {
        current.push(component);
        let metadata = current.symlink_metadata()?;
        if metadata.file_type().is_symlink() {
            return Err(io::Error::other("symlink detected"));
        }
    }
    fs::OpenOptions::new().read(true).open(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_port() -> (tempfile::TempDir, CompressionContentPort) {
        let dir = tempfile::tempdir().unwrap();
        let canonical = dir.path().canonicalize().unwrap();
        let port = CompressionContentPort::new(canonical).unwrap();
        (dir, port)
    }

    #[test]
    fn new_rejects_missing_root() {
        let dir = tempfile::tempdir().unwrap();
        assert!(CompressionContentPort::new(dir.path().join("missing")).is_none());
    }

    #[test]
    fn new_rejects_file_root() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("root-file");
        fs::write(&file, b"not a directory").unwrap();
        assert!(CompressionContentPort::new(file).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn new_rejects_symlink_root() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real-root");
        let link = dir.path().join("root-link");
        fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert!(CompressionContentPort::new(link).is_none());
    }

    #[test]
    fn resolve_reads_file_within_jail() {
        let (dir, port) = make_port();
        fs::write(dir.path().join("hello.txt"), b"world").unwrap();
        let data = port.resolve("file:hello.txt").unwrap();
        assert_eq!(data, b"world");
    }

    #[test]
    fn resolve_rejects_traversal() {
        let (_dir, port) = make_port();
        let err = port.resolve("file:../etc/passwd").unwrap_err();
        assert!(err.to_string().contains("jail"));
    }

    #[test]
    fn resolve_rejects_oversized_file() {
        let (dir, port) = make_port();
        let big = vec![0u8; MAX_CONTENT_BYTES + 1];
        fs::write(dir.path().join("big.bin"), &big).unwrap();
        let err = port.resolve("file:big.bin").unwrap_err();
        assert!(err.to_string().contains("limit"));
    }

    #[test]
    fn resolve_requires_file_scheme() {
        let (_dir, port) = make_port();
        let err = port.resolve("http://evil.com").unwrap_err();
        assert!(err.to_string().contains("file: scheme"));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_rejects_symlink_final_component() {
        let (dir, port) = make_port();
        let root = dir.path().canonicalize().unwrap();
        fs::write(root.join("real.txt"), b"secret").unwrap();
        std::os::unix::fs::symlink(root.join("real.txt"), root.join("link.txt")).unwrap();

        let err = port.resolve("file:link.txt").unwrap_err();
        assert!(
            err.to_string().contains("symlink"),
            "expected symlink rejection, got: {}",
            err
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_rejects_symlink_intermediate_dir() {
        let (dir, port) = make_port();
        let root = dir.path().canonicalize().unwrap();
        let real_dir = root.join("real_dir");
        fs::create_dir(&real_dir).unwrap();
        fs::write(real_dir.join("target.txt"), b"hidden").unwrap();
        std::os::unix::fs::symlink(&real_dir, root.join("sym_dir")).unwrap();

        let result = port.resolve("file:sym_dir/target.txt");
        assert!(result.is_err(), "should reject symlinked intermediate dir");
        assert!(
            result.unwrap_err().to_string().contains("symlink"),
            "error should mention symlink"
        );
    }

    #[cfg(unix)]
    #[test]
    fn resolve_survives_intermediate_directory_symlink_race() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        use std::thread;

        let (dir, port) = make_port();
        let root = dir.path().canonicalize().unwrap();
        let outside = dir.path().join("outside");
        let safe_a = root.join("safe-a");
        let branch = root.join("branch");
        fs::create_dir_all(&safe_a).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(safe_a.join("target.txt"), b"safe").unwrap();
        fs::write(outside.join("target.txt"), b"secret").unwrap();
        std::os::unix::fs::symlink(&outside, &branch).unwrap();

        let stop = Arc::new(AtomicBool::new(false));
        let attacker_stop = Arc::clone(&stop);
        let attacker = thread::spawn(move || {
            while !attacker_stop.load(Ordering::Relaxed) {
                let _ = fs::remove_dir(&branch);
                let _ = fs::remove_file(&branch);
                let _ = fs::rename(&safe_a, &branch);
                let _ = fs::rename(&branch, &safe_a);
                let _ = fs::remove_dir(&branch);
                let _ = fs::remove_file(&branch);
                let _ = std::os::unix::fs::symlink(&outside, &branch);
            }
        });

        for _ in 0..2_000 {
            if let Ok(data) = port.resolve("file:branch/target.txt") {
                assert_eq!(data, b"safe");
            }
        }
        stop.store(true, Ordering::Relaxed);
        attacker.join().unwrap();
    }

    #[test]
    fn persist_and_retrieve_roundtrip() {
        let (_dir, port) = make_port();
        let data = b"compressed content";
        let ref_key = port.persist(data).unwrap();
        assert!(ref_key.starts_with("blake3:"));
        let retrieved = port.retrieve(&ref_key).unwrap();
        assert_eq!(retrieved, data);
    }

    #[test]
    fn persist_deduplicates() {
        let (_dir, port) = make_port();
        let data = b"same content";
        let ref1 = port.persist(data).unwrap();
        let ref2 = port.persist(data).unwrap();
        assert_eq!(ref1, ref2);
    }

    #[test]
    fn cache_evicts_when_full() {
        let (_dir, port) = make_port();
        for i in 0..MAX_CACHE_ENTRIES + 10 {
            let data = format!("entry-{i}");
            port.persist(data.as_bytes()).unwrap();
        }
        let cache = port.cache.lock().unwrap();
        assert!(cache.entries.len() <= MAX_CACHE_ENTRIES);
    }
}
