use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{assert_file_invariants, FileRecord, ModelError, VersionId, VersionRecord};

/// Retention policy for automatic version window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionRetention {
    /// Keep at most this many versions (always keeps current head).
    pub max_versions: usize,
    /// Optionally drop versions older than this age (relative to now).
    pub max_age: Option<Duration>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VersioningError {
    #[error("version {0} not found")]
    MissingVersion(VersionId),
    #[error(transparent)]
    Model(#[from] ModelError),
}

/// List versions ordered as stored (usually insertion order).
pub fn list_versions(file: &FileRecord) -> &[VersionRecord] {
    &file.versions
}

/// Create a rollback version that points to a previous version and make it the head.
///
/// Caller provides the new VersionRecord (with content hash/chunks for the restored data).
/// This ensures the target exists and updates head, preserving history.
pub fn rollback_to_version(
    file: &mut FileRecord,
    target_version_id: VersionId,
    new_version: VersionRecord,
) -> Result<(), VersioningError> {
    if !file.versions.iter().any(|v| v.version_id == target_version_id) {
        return Err(VersioningError::MissingVersion(target_version_id));
    }

    file.versions.push(new_version.clone());
    file.head_version_id = new_version.version_id;
    assert_file_invariants(file)?;
    Ok(())
}

/// Apply retention: keeps head, then prunes by count and age.
pub fn apply_retention(
    file: &mut FileRecord,
    policy: &VersionRetention,
    now: SystemTime,
) -> Result<(), VersioningError> {
    // Always preserve the head version.
    let head_id = file.head_version_id;

    // Filter by age first if configured.
    if let Some(max_age) = policy.max_age {
        let cutoff = now
            .checked_sub(max_age)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let cutoff: DateTime<Utc> = DateTime::from(cutoff);
        file.versions
            .retain(|v| v.version_id == head_id || v.timestamp >= cutoff);
    }

    // Enforce max_versions (including head).
    if file.versions.len() > policy.max_versions {
        // Keep head plus most recent others by timestamp.
        file.versions.sort_by_key(|v| v.timestamp);
        let keep_from = file
            .versions
            .len()
            .saturating_sub(policy.max_versions);
        let cutoff_ts = file.versions[keep_from].timestamp;
        file.versions
            .retain(|v| v.version_id == head_id || v.timestamp >= cutoff_ts);
    }

    assert_file_invariants(file)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChunkRef, EncryptionInfo};
    use chrono::{Duration as ChronoDuration, Utc};

    fn ulid() -> VersionId {
        ulid::Ulid::new()
    }

    fn sample_file_with_versions(count: usize) -> FileRecord {
        let file_id = ulid();
        let mut versions = Vec::new();
        let mut head = None;
        for i in 0..count {
            let vid = ulid();
            head = Some(vid);
            versions.push(VersionRecord {
                version_id: vid,
                file_id,
                parent_version_id: None,
                origin_device_id: ulid(),
                timestamp: (Utc::now() - ChronoDuration::seconds((count - i) as i64)).into(),
                content_hash: format!("h{i}"),
                size_bytes: 1,
                chunks: vec![ChunkRef {
                    offset: 0,
                    length: 1,
                    hash: format!("h{i}"),
                }],
            });
        }

        FileRecord {
            file_id,
            origin_device_id: ulid(),
            created_at: Utc::now(),
            head_version_id: head.unwrap(),
            versions,
            lock: None,
            device_states: vec![],
            encryption: EncryptionInfo {
                key_id: "k".into(),
                algo: "AES-256-GCM".into(),
                iv_salt: None,
            },
        }
    }

    #[test]
    fn rollback_adds_new_head() {
        let mut file = sample_file_with_versions(2);
        let target = file.versions[0].version_id;
        let restore_version = VersionRecord {
            version_id: ulid(),
            file_id: file.file_id,
            parent_version_id: Some(target),
            origin_device_id: ulid(),
            timestamp: SystemTime::now().into(),
            content_hash: "restored".into(),
            size_bytes: 1,
            chunks: file.versions[0].chunks.clone(),
        };
        rollback_to_version(&mut file, target, restore_version).unwrap();
        assert_eq!(file.head_version_id, file.versions.last().unwrap().version_id);
    }

    #[test]
    fn retention_limits_versions() {
        let mut file = sample_file_with_versions(5);
        let policy = VersionRetention {
            max_versions: 3,
            max_age: None,
        };
        apply_retention(&mut file, &policy, SystemTime::now()).unwrap();
        assert!(file.versions.len() <= 3);
        assert!(file.versions.iter().any(|v| v.version_id == file.head_version_id));
    }
}
