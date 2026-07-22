//! Kernel state serialization, snapshots, and recovery.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SNAPSHOT_VERSION: u32 = 1;

/// Durable state needed to resume kernel operation after a restart.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct KernelSnapshot {
    pub version: u32,
    pub timestamp_epoch: u64,
    pub provider_weights: HashMap<String, f64>,
    pub recent_plan_ids: Vec<String>,
    pub recent_receipt_ids: Vec<String>,
    pub degradation_level: String,
    pub circuit_states: HashMap<String, String>,
}

impl KernelSnapshot {
    /// Capture the current recoverable kernel state.
    pub fn capture(
        provider_weights: &HashMap<String, f64>,
        recent_plans: &[String],
        recent_receipts: &[String],
        degradation: &str,
        circuits: &HashMap<String, String>,
    ) -> Self {
        let timestamp_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs());

        Self {
            version: SNAPSHOT_VERSION,
            timestamp_epoch,
            provider_weights: provider_weights.clone(),
            recent_plan_ids: recent_plans.to_vec(),
            recent_receipt_ids: recent_receipts.to_vec(),
            degradation_level: degradation.to_owned(),
            circuit_states: circuits.clone(),
        }
    }
}

/// Errors encountered while persisting or recovering a kernel snapshot.
#[derive(Debug)]
pub enum SnapshotError {
    Io(std::io::Error),
    Serialize(String),
    Deserialize(String),
    InvalidVersion(u32),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "snapshot I/O error: {error}"),
            Self::Serialize(message) => {
                write!(f, "snapshot serialization error: {message}")
            }
            Self::Deserialize(message) => {
                write!(f, "snapshot deserialization error: {message}")
            }
            Self::InvalidVersion(version) => {
                write!(f, "unsupported snapshot version: {version}")
            }
        }
    }
}

impl std::error::Error for SnapshotError {}

impl From<std::io::Error> for SnapshotError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Persist a snapshot using a temporary sibling file and atomic rename.
pub fn save_snapshot(snapshot: &KernelSnapshot, path: &Path) -> Result<(), SnapshotError> {
    let serialized = serde_json::to_vec_pretty(snapshot)
        .map_err(|error| SnapshotError::Serialize(error.to_string()))?;
    let temporary_path = temporary_path(path);

    if let Err(error) = std::fs::write(&temporary_path, serialized) {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(SnapshotError::Io(error));
    }

    if let Err(error) = std::fs::rename(&temporary_path, path) {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(SnapshotError::Io(error));
    }

    Ok(())
}

/// Load and validate a persisted kernel snapshot.
pub fn load_snapshot(path: &Path) -> Result<KernelSnapshot, SnapshotError> {
    let serialized = std::fs::read(path)?;
    let snapshot: KernelSnapshot = serde_json::from_slice(&serialized)
        .map_err(|error| SnapshotError::Deserialize(error.to_string()))?;

    if snapshot.version != SNAPSHOT_VERSION {
        return Err(SnapshotError::InvalidVersion(snapshot.version));
    }

    Ok(snapshot)
}

/// Return the standard per-user location for kernel recovery state.
pub fn default_snapshot_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("lean-ctx")
        .join("kernel")
        .join("snapshot.json")
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".tmp.{}", std::process::id()));
    PathBuf::from(temporary)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use super::{KernelSnapshot, SnapshotError, load_snapshot, save_snapshot, temporary_path};

    fn sample_snapshot() -> KernelSnapshot {
        let provider_weights = HashMap::from([
            ("filesystem".to_owned(), 0.75),
            ("knowledge".to_owned(), 1.25),
        ]);
        let circuit_states = HashMap::from([
            ("filesystem".to_owned(), "closed".to_owned()),
            ("knowledge".to_owned(), "half_open".to_owned()),
        ]);

        KernelSnapshot {
            version: 1,
            timestamp_epoch: 1_700_000_000,
            provider_weights,
            recent_plan_ids: vec!["plan-1".to_owned(), "plan-2".to_owned()],
            recent_receipt_ids: vec!["receipt-1".to_owned()],
            degradation_level: "normal".to_owned(),
            circuit_states,
        }
    }

    #[test]
    fn save_load_roundtrip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snapshot.json");
        let snapshot = sample_snapshot();

        save_snapshot(&snapshot, &path).unwrap();
        let loaded = load_snapshot(&path).unwrap();

        assert_eq!(loaded, snapshot);
        assert!(!temporary_path(&path).exists());
    }

    #[test]
    fn invalid_version_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snapshot.json");
        let mut snapshot = sample_snapshot();
        snapshot.version = 99;
        save_snapshot(&snapshot, &path).unwrap();

        let error = load_snapshot(&path).unwrap_err();

        assert!(matches!(error, SnapshotError::InvalidVersion(99)));
    }

    #[test]
    fn missing_file_returns_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("missing.json");

        let error = load_snapshot(&path).unwrap_err();

        assert!(matches!(error, SnapshotError::Io(_)));
    }

    #[test]
    fn atomic_write_no_partial() {
        let directory = tempfile::tempdir().unwrap();
        let missing_directory = directory.path().join("missing");
        let path = missing_directory.join("snapshot.json");

        let error = save_snapshot(&sample_snapshot(), &path).unwrap_err();

        assert!(matches!(error, SnapshotError::Io(_)));
        assert!(!Path::new(&path).exists());
        assert!(!temporary_path(&path).exists());
    }
}
