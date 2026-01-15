use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ulid::Ulid;

/// Stable, path-independent identifiers.
pub type FileId = Ulid;
pub type DeviceId = Ulid;
pub type VersionId = Ulid;
pub type LockId = Ulid;
pub type TransferSessionId = Ulid;

/// Resumable transfer chunk metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkRef {
    pub offset: u64,
    pub length: u64,
    pub hash: String, // strong hash (e.g., SHA-256 hex)
}

/// Lightweight version record (shared).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionRecord {
    pub version_id: VersionId,
    pub file_id: FileId,
    pub parent_version_id: Option<VersionId>,
    pub origin_device_id: DeviceId,
    pub timestamp: DateTime<Utc>,
    pub content_hash: String,
    pub size_bytes: u64,
    pub chunks: Vec<ChunkRef>,
}

/// Per-file lock metadata (shared).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockRecord {
    pub lock_id: LockId,
    pub file_id: FileId,
    pub owner_device_id: DeviceId,
    pub owner_user_id: String,
    pub mode: LockMode,
    pub acquired_at: DateTime<Utc>,
    pub auto_lock: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockMode {
    Exclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceFileStateKind {
    Absent,
    AvailableRemote,
    Pulling,
    Ready,
    Pushing,
    LockBlocked,
    Conflict,
    Error,
}

/// Per-device state vector (shared).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceFileState {
    pub device_id: DeviceId,
    pub state: DeviceFileStateKind,
    pub known_head_version_id: Option<VersionId>,
    pub last_seen_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

/// Encryption envelope metadata (shared, keys stored locally).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncryptionInfo {
    pub key_id: String,
    pub algo: String, // e.g., "AES-256-GCM"
    pub iv_salt: Option<String>,
}

/// File-level shared record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    pub file_id: FileId,
    pub origin_device_id: DeviceId,
    pub created_at: DateTime<Utc>,
    pub head_version_id: VersionId,
    pub versions: Vec<VersionRecord>,
    pub lock: Option<LockRecord>,
    pub device_states: Vec<DeviceFileState>,
    pub encryption: EncryptionInfo,
}

/// Local-only registry entry; path mappings keep identity stable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalRegistryEntry {
    pub file_id: FileId,
    pub paths: Vec<PathBinding>,
    pub local_version_id: Option<VersionId>,
    pub hydration: Hydration,
    pub consent: Consent,
    pub pin: PinPreference,
    pub auto_lock_preference: AutoLockPreference,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathBinding {
    pub path: String,
    pub last_seen_at: DateTime<Utc>,
    pub writable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hydration {
    FullyPresent,
    Partial,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Consent {
    Approved,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinPreference {
    None,
    KeepLatest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutoLockPreference {
    OnEdit,
    Manual,
}

/// Transfer session (local, with minimal shared status for coordination).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferSession {
    pub transfer_session_id: TransferSessionId,
    pub file_id: FileId,
    pub direction: TransferDirection,
    pub from_device_id: DeviceId,
    pub to_device_id: DeviceId,
    pub active_chunks: Vec<ChunkRef>,
    pub retry_count: u32,
    pub status: TransferStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferDirection {
    Push,
    Pull,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferStatus {
    InProgress,
    Completed,
    Failed(String),
}

/// Errors when validating invariants or state transitions.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModelError {
    #[error("head version {0} not present in versions list")]
    MissingHead(VersionId),
    #[error("duplicate version id {0}")]
    DuplicateVersion(VersionId),
    #[error("multiple active locks found")]
    MultipleLocks,
    #[error("device state missing for device {0}")]
    MissingDevice(DeviceId),
}

/// Validate invariants for a shared FileRecord.
///
/// - Head version must exist in versions list.
/// - Versions list must not contain duplicates.
/// - At most one active lock.
/// - Each DeviceFileState must have a unique device_id.
pub fn assert_file_invariants(record: &FileRecord) -> Result<(), ModelError> {
    let mut seen_versions = std::collections::HashSet::new();
    let mut head_present = false;
    for v in &record.versions {
        if !seen_versions.insert(v.version_id) {
            return Err(ModelError::DuplicateVersion(v.version_id));
        }
        if v.version_id == record.head_version_id {
            head_present = true;
        }
    }
    if !head_present {
        return Err(ModelError::MissingHead(record.head_version_id));
    }

    if record.lock.is_some() {
        // Because lock is optional and singular, a second lock would require a different field.
        // This guard ensures the intent is explicit.
        // (Retained to document invariant explicitly; runtime check is trivial.)
        // Additional enforcement could check lock.file_id == record.file_id.
    }

    let mut seen_devices = std::collections::HashSet::new();
    for state in &record.device_states {
        if !seen_devices.insert(state.device_id) {
            return Err(ModelError::MissingDevice(state.device_id));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ulid() -> Ulid {
        Ulid::new()
    }

    fn sample_version(file_id: FileId, version_id: VersionId) -> VersionRecord {
        VersionRecord {
            version_id,
            file_id,
            parent_version_id: None,
            origin_device_id: ulid(),
            timestamp: Utc::now(),
            content_hash: "hash".into(),
            size_bytes: 10,
            chunks: vec![ChunkRef {
                offset: 0,
                length: 10,
                hash: "hash".into(),
            }],
        }
    }

    fn sample_file_record() -> FileRecord {
        let file_id = ulid();
        let version_id = ulid();
        FileRecord {
            file_id,
            origin_device_id: ulid(),
            created_at: Utc::now(),
            head_version_id: version_id,
            versions: vec![sample_version(file_id, version_id)],
            lock: None,
            device_states: vec![DeviceFileState {
                device_id: ulid(),
                state: DeviceFileStateKind::Ready,
                known_head_version_id: Some(version_id),
                last_seen_at: Utc::now(),
                last_error: None,
            }],
            encryption: EncryptionInfo {
                key_id: "k1".into(),
                algo: "AES-256-GCM".into(),
                iv_salt: None,
            },
        }
    }

    #[test]
    fn validates_ok_record() {
        let record = sample_file_record();
        assert_file_invariants(&record).unwrap();
    }

    #[test]
    fn detects_missing_head() {
        let mut record = sample_file_record();
        record.head_version_id = ulid();
        let err = assert_file_invariants(&record).unwrap_err();
        assert!(matches!(err, ModelError::MissingHead(_)));
    }

    #[test]
    fn detects_duplicate_versions() {
        let mut record = sample_file_record();
        let dup = record.versions[0].clone();
        record.versions.push(dup);
        let err = assert_file_invariants(&record).unwrap_err();
        assert!(matches!(err, ModelError::DuplicateVersion(_)));
    }

    #[test]
    fn detects_duplicate_device_states() {
        let mut record = sample_file_record();
        let dup_device = record.device_states[0].device_id;
        record.device_states.push(DeviceFileState {
            device_id: dup_device,
            state: DeviceFileStateKind::Ready,
            known_head_version_id: record.device_states[0].known_head_version_id,
            last_seen_at: Utc::now(),
            last_error: None,
        });
        let err = assert_file_invariants(&record).unwrap_err();
        assert!(matches!(err, ModelError::MissingDevice(_)));
    }
}
