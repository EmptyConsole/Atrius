use chrono::{DateTime, Utc};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ulid::Ulid;

use crate::{DeviceFileStateKind, DeviceId, FileRecord, LockMode, LockRecord, VersionId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockRequestKind {
    Manual,
    Auto,
}

/// Result of attempting to acquire a lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockAcquisition {
    Acquired(LockRecord),
    Denied(LockDenial),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockDenial {
    pub holder_device: DeviceId,
    pub acquired_at: DateTime<Utc>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LockError {
    #[error("file record not found")]
    MissingFile,
    #[error("lock mismatch: existing lock for a different file")]
    LockMismatch,
}

/// Attempt to acquire an exclusive lock for a device. If a lock exists, it is respected.
pub fn acquire_lock(
    file: &FileRecord,
    device_id: DeviceId,
    user_id: String,
    _request: LockRequestKind,
    auto_lock: bool,
) -> Result<LockAcquisition, LockError> {
    if let Some(lock) = &file.lock {
        if lock.file_id != file.file_id {
            return Err(LockError::LockMismatch);
        }
        return Ok(LockAcquisition::Denied(LockDenial {
            holder_device: lock.owner_device_id,
            acquired_at: lock.acquired_at,
        }));
    }

    let record = LockRecord {
        lock_id: Ulid::new(),
        file_id: file.file_id,
        owner_device_id: device_id,
        owner_user_id: user_id,
        mode: LockMode::Exclusive,
        acquired_at: Utc::now(),
        auto_lock,
        expires_at: None,
    };

    Ok(LockAcquisition::Acquired(record))
}

/// Release a lock if held by the device; otherwise no-op.
pub fn release_lock(file: &mut FileRecord, device_id: DeviceId) -> Result<(), LockError> {
    if let Some(lock) = &file.lock {
        if lock.file_id != file.file_id {
            return Err(LockError::LockMismatch);
        }
        if lock.owner_device_id == device_id {
            file.lock = None;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictCheck {
    Allowed,
    Conflict { current_head: VersionId, base_head: VersionId },
    LockedBy(DeviceId),
}

/// Simple conflict rule:
/// - If lock is held by caller -> allowed.
/// - If lock held by other -> LockedBy.
/// - If no lock: require pushes to base on current head; else Conflict.
pub fn check_conflict(
    file: &FileRecord,
    caller_device: DeviceId,
    caller_base_head: VersionId,
) -> ConflictCheck {
    if let Some(lock) = &file.lock {
        if lock.owner_device_id == caller_device {
            return ConflictCheck::Allowed;
        } else {
            return ConflictCheck::LockedBy(lock.owner_device_id);
        }
    }

    if caller_base_head == file.head_version_id {
        ConflictCheck::Allowed
    } else {
        ConflictCheck::Conflict {
            current_head: file.head_version_id,
            base_head: caller_base_head,
        }
    }
}

/// Update per-device state to reflect lock blocked status.
pub fn mark_lock_blocked(file: &mut FileRecord, device_id: DeviceId) {
    if let Some(state) = file
        .device_states
        .iter_mut()
        .find(|s| s.device_id == device_id)
    {
        state.state = DeviceFileStateKind::LockBlocked;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChunkRef, DeviceFileState, EncryptionInfo, VersionRecord};
    use chrono::Utc;

    fn sample_file() -> FileRecord {
        let file_id = Ulid::new();
        let head = Ulid::new();
        FileRecord {
            file_id,
            origin_device_id: Ulid::new(),
            created_at: Utc::now(),
            head_version_id: head,
            versions: vec![VersionRecord {
                version_id: head,
                file_id,
                parent_version_id: None,
                origin_device_id: Ulid::new(),
                timestamp: Utc::now(),
                content_hash: "h".into(),
                size_bytes: 1,
                chunks: vec![ChunkRef {
                    offset: 0,
                    length: 1,
                    hash: "h".into(),
                }],
            }],
            lock: None,
            device_states: vec![DeviceFileState {
                device_id: Ulid::new(),
                state: DeviceFileStateKind::Ready,
                known_head_version_id: Some(head),
                last_seen_at: Utc::now(),
                last_error: None,
            }],
            encryption: EncryptionInfo {
                key_id: "k".into(),
                algo: "AES-256-GCM".into(),
                iv_salt: None,
            },
        }
    }

    #[test]
    fn acquires_when_unlocked() {
        let file = sample_file();
        let device = Ulid::new();
        let res = acquire_lock(
            &file,
            device,
            "user".into(),
            LockRequestKind::Manual,
            false,
        )
        .unwrap();
        matches!(res, LockAcquisition::Acquired(_));
    }

    #[test]
    fn denies_when_locked_by_other() {
        let file = sample_file();
        let device_a = Ulid::new();
        let device_b = Ulid::new();
        let lock = acquire_lock(
            &file,
            device_a,
            "user".into(),
            LockRequestKind::Manual,
            false,
        )
        .unwrap();
        if let LockAcquisition::Acquired(lock) = lock {
            let mut file_mut = file.clone();
            file_mut.lock = Some(lock);
            let denied = acquire_lock(
                &file_mut,
                device_b,
                "user2".into(),
                LockRequestKind::Manual,
                false,
            )
            .unwrap();
            assert!(matches!(denied, LockAcquisition::Denied(_)));
        }
    }

    #[test]
    fn conflict_when_head_diverges_without_lock() {
        let file = sample_file();
        let caller_base = Ulid::new();
        let res = check_conflict(&file, Ulid::new(), caller_base);
        assert!(matches!(
            res,
            ConflictCheck::Conflict { current_head: _, base_head: _ }
        ));
    }

    #[test]
    fn allowed_when_head_matches_no_lock() {
        let file = sample_file();
        let res = check_conflict(&file, Ulid::new(), file.head_version_id);
        assert!(matches!(res, ConflictCheck::Allowed));
    }

    #[test]
    fn locked_by_other_blocks() {
        let file = sample_file();
        let device_a = Ulid::new();
        if let LockAcquisition::Acquired(lock) =
            acquire_lock(&file, device_a, "u".into(), LockRequestKind::Manual, false).unwrap()
        {
            let mut f = file.clone();
            f.lock = Some(lock);
            let res = check_conflict(&f, Ulid::new(), f.head_version_id);
            assert!(matches!(res, ConflictCheck::LockedBy(_)));
        }
    }
}
