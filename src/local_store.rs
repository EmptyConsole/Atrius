use std::collections::HashMap;

use chrono::Utc;
use thiserror::Error;

use crate::{
    assert_file_invariants, AutoLockPreference, Consent, DeviceFileState, FileId, FileRecord,
    Hydration, LocalRegistryEntry, ModelError, PathBinding, VersionId,
};

/// In-memory local metadata store. This tracks file identities, shared metadata snapshots,
/// and local registry info without assuming ownership of any folders.
///
/// Persistence is intentionally abstracted; callers can serialize/deserialize the store or
/// rehydrate from a DB of their choice (e.g., SQLite) using the public accessors.
#[derive(Default, Debug)]
pub struct LocalMetadataStore {
    files: HashMap<FileId, FileRecord>,
    registry: HashMap<FileId, LocalRegistryEntry>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LocalMetadataError {
    #[error("file {0} not found")]
    NotFound(FileId),
    #[error("path already bound to file {0}")]
    PathAlreadyBound(FileId),
    #[error(transparent)]
    Model(#[from] ModelError),
}

impl LocalMetadataStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or replace a `FileRecord` after validating invariants.
    pub fn upsert_file_record(&mut self, record: FileRecord) -> Result<(), LocalMetadataError> {
        assert_file_invariants(&record)?;
        self.files.insert(record.file_id, record);
        Ok(())
    }

    /// Insert or replace the local registry entry for a file.
    pub fn upsert_registry_entry(
        &mut self,
        entry: LocalRegistryEntry,
    ) -> Result<(), LocalMetadataError> {
        self.registry.insert(entry.file_id, entry);
        Ok(())
    }

    /// Bind or update a path for a file without changing identity.
    pub fn bind_path(
        &mut self,
        file_id: FileId,
        path: String,
        writable: bool,
    ) -> Result<(), LocalMetadataError> {
        // Prevent binding the same path to multiple FileIds.
        if let Some(conflict_id) = self.registry.iter().find_map(|(other_id, other_entry)| {
            if *other_id != file_id
                && other_entry
                    .paths
                    .iter()
                    .any(|p| p.path.eq_ignore_ascii_case(&path))
            {
                Some(*other_id)
            } else {
                None
            }
        }) {
            return Err(LocalMetadataError::PathAlreadyBound(conflict_id));
        }

        let entry = self
            .registry
            .get_mut(&file_id)
            .ok_or(LocalMetadataError::NotFound(file_id))?;

        if let Some(existing) = entry.paths.iter_mut().find(|p| p.path == path) {
            existing.last_seen_at = Utc::now();
            existing.writable = writable;
        } else {
            entry.paths.push(PathBinding {
                path,
                last_seen_at: Utc::now(),
                writable,
            });
        }
        Ok(())
    }

    /// Remove a path binding; identity remains intact.
    pub fn unbind_path(&mut self, file_id: FileId, path: &str) -> Result<(), LocalMetadataError> {
        let entry = self
            .registry
            .get_mut(&file_id)
            .ok_or(LocalMetadataError::NotFound(file_id))?;
        entry.paths.retain(|p| p.path != path);
        Ok(())
    }

    /// Update local hydration/consent/auto-lock knobs.
    pub fn set_local_preferences(
        &mut self,
        file_id: FileId,
        hydration: Option<Hydration>,
        consent: Option<Consent>,
        auto_lock: Option<AutoLockPreference>,
    ) -> Result<(), LocalMetadataError> {
        let entry = self
            .registry
            .get_mut(&file_id)
            .ok_or(LocalMetadataError::NotFound(file_id))?;
        if let Some(h) = hydration {
            entry.hydration = h;
        }
        if let Some(c) = consent {
            entry.consent = c;
        }
        if let Some(a) = auto_lock {
            entry.auto_lock_preference = a;
        }
        Ok(())
    }

    /// Add or update a device state in the shared record.
    pub fn upsert_device_state(
        &mut self,
        file_id: FileId,
        device_state: DeviceFileState,
    ) -> Result<(), LocalMetadataError> {
        let record = self
            .files
            .get_mut(&file_id)
            .ok_or(LocalMetadataError::NotFound(file_id))?;

        if let Some(existing) = record
            .device_states
            .iter_mut()
            .find(|d| d.device_id == device_state.device_id)
        {
            *existing = device_state;
        } else {
            record.device_states.push(device_state);
        }
        assert_file_invariants(record)?;
        Ok(())
    }

    /// Advance head to a new version and append it to versions.
    pub fn append_version(
        &mut self,
        file_id: FileId,
        version_id: VersionId,
        version_record: crate::VersionRecord,
    ) -> Result<(), LocalMetadataError> {
        let record = self
            .files
            .get_mut(&file_id)
            .ok_or(LocalMetadataError::NotFound(file_id))?;
        record.head_version_id = version_id;
        record.versions.push(version_record);
        assert_file_invariants(record)?;
        if let Some(entry) = self.registry.get_mut(&file_id) {
            entry.local_version_id = Some(version_id);
        }
        Ok(())
    }

    /// Mark lock status on the shared record.
    pub fn set_lock(
        &mut self,
        file_id: FileId,
        lock: Option<crate::LockRecord>,
    ) -> Result<(), LocalMetadataError> {
        let record = self
            .files
            .get_mut(&file_id)
            .ok_or(LocalMetadataError::NotFound(file_id))?;
        record.lock = lock;
        assert_file_invariants(record)?;
        Ok(())
    }

    /// Update local last error for visibility without affecting shared metadata.
    pub fn set_local_error(
        &mut self,
        file_id: FileId,
        message: Option<String>,
    ) -> Result<(), LocalMetadataError> {
        let entry = self
            .registry
            .get_mut(&file_id)
            .ok_or(LocalMetadataError::NotFound(file_id))?;
        entry.last_error = message;
        Ok(())
    }

    /// Getters for persistence/export.
    pub fn file_record(&self, file_id: &FileId) -> Option<&FileRecord> {
        self.files.get(file_id)
    }

    pub fn registry_entry(&self, file_id: &FileId) -> Option<&LocalRegistryEntry> {
        self.registry.get(file_id)
    }

    pub fn files(&self) -> impl Iterator<Item = &FileRecord> {
        self.files.values()
    }

    pub fn registry_entries(&self) -> impl Iterator<Item = &LocalRegistryEntry> {
        self.registry.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ChunkRef, DeviceFileStateKind, EncryptionInfo, LockMode, LockRecord, VersionRecord,
    };
    use chrono::Duration;

    fn ulid() -> crate::FileId {
        ulid::Ulid::new()
    }

    fn sample_file_record() -> FileRecord {
        let file_id = ulid();
        let version_id = ulid();
        FileRecord {
            file_id,
            origin_device_id: ulid(),
            created_at: Utc::now(),
            head_version_id: version_id,
            versions: vec![VersionRecord {
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
            }],
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

    fn sample_registry_entry(file_id: FileId) -> LocalRegistryEntry {
        LocalRegistryEntry {
            file_id,
            paths: vec![PathBinding {
                path: "/tmp/a".into(),
                last_seen_at: Utc::now(),
                writable: true,
            }],
            local_version_id: None,
            hydration: Hydration::FullyPresent,
            consent: Consent::Approved,
            pin: crate::PinPreference::None,
            auto_lock_preference: AutoLockPreference::OnEdit,
            last_error: None,
        }
    }

    #[test]
    fn upsert_and_bind_paths_without_changing_identity() {
        let mut store = LocalMetadataStore::new();
        let record = sample_file_record();
        let file_id = record.file_id;
        store.upsert_file_record(record).unwrap();
        store
            .upsert_registry_entry(sample_registry_entry(file_id))
            .unwrap();

        store
            .bind_path(file_id, "/tmp/renamed".into(), true)
            .unwrap();
        let entry = store.registry_entry(&file_id).unwrap();
        assert!(entry.paths.iter().any(|p| p.path == "/tmp/renamed"));
    }

    #[test]
    fn prevents_path_alias_across_files() {
        let mut store = LocalMetadataStore::new();
        let r1 = sample_file_record();
        let r2 = sample_file_record();
        let f1 = r1.file_id;
        let f2 = r2.file_id;
        store.upsert_file_record(r1).unwrap();
        store.upsert_file_record(r2).unwrap();
        store
            .upsert_registry_entry(sample_registry_entry(f1))
            .unwrap();
        store
            .upsert_registry_entry(sample_registry_entry(f2))
            .unwrap();

        let err = store
            .bind_path(f2, "/tmp/a".into(), true)
            .expect_err("should reject alias");
        assert!(matches!(err, LocalMetadataError::PathAlreadyBound(id) if id == f1));
    }

    #[test]
    fn updates_device_state_and_keeps_invariants() {
        let mut store = LocalMetadataStore::new();
        let record = sample_file_record();
        let file_id = record.file_id;
        let device_id = record.device_states[0].device_id;
        store.upsert_file_record(record.clone()).unwrap();

        store
            .upsert_device_state(
                file_id,
                DeviceFileState {
                    device_id,
                    state: DeviceFileStateKind::Pushing,
                    known_head_version_id: record.device_states[0].known_head_version_id,
                    last_seen_at: Utc::now() + Duration::seconds(1),
                    last_error: None,
                },
            )
            .unwrap();

        let updated = store.file_record(&file_id).unwrap();
        assert_eq!(
            updated
                .device_states
                .iter()
                .find(|d| d.device_id == device_id)
                .unwrap()
                .state,
            DeviceFileStateKind::Pushing
        );
    }

    #[test]
    fn sets_and_clears_lock() {
        let mut store = LocalMetadataStore::new();
        let record = sample_file_record();
        let file_id = record.file_id;
        store.upsert_file_record(record).unwrap();

        store
            .set_lock(
                file_id,
                Some(LockRecord {
                    lock_id: ulid(),
                    file_id,
                    owner_device_id: ulid(),
                    owner_user_id: "user".into(),
                    mode: LockMode::Exclusive,
                    acquired_at: Utc::now(),
                    auto_lock: true,
                    expires_at: None,
                }),
            )
            .unwrap();

        assert!(store.file_record(&file_id).unwrap().lock.is_some());
        store.set_lock(file_id, None).unwrap();
        assert!(store.file_record(&file_id).unwrap().lock.is_none());
    }

    #[test]
    fn append_version_updates_head_and_registry() {
        let mut store = LocalMetadataStore::new();
        let record = sample_file_record();
        let file_id = record.file_id;
        store.upsert_file_record(record).unwrap();
        store
            .upsert_registry_entry(sample_registry_entry(file_id))
            .unwrap();

        let new_version_id = ulid();
        store
            .append_version(
                file_id,
                new_version_id,
                VersionRecord {
                    version_id: new_version_id,
                    file_id,
                    parent_version_id: None,
                    origin_device_id: ulid(),
                    timestamp: Utc::now(),
                    content_hash: "hash2".into(),
                    size_bytes: 20,
                    chunks: vec![ChunkRef {
                        offset: 0,
                        length: 20,
                        hash: "hash2".into(),
                    }],
                },
            )
            .unwrap();

        let updated = store.file_record(&file_id).unwrap();
        assert_eq!(updated.head_version_id, new_version_id);
        assert_eq!(updated.versions.len(), 2);
        assert_eq!(
            store.registry_entry(&file_id).unwrap().local_version_id,
            Some(new_version_id)
        );
    }

    #[test]
    fn set_local_preferences_updates_flags() {
        let mut store = LocalMetadataStore::new();
        let record = sample_file_record();
        let file_id = record.file_id;
        store.upsert_file_record(record).unwrap();
        store
            .upsert_registry_entry(sample_registry_entry(file_id))
            .unwrap();

        store
            .set_local_preferences(
                file_id,
                Some(Hydration::None),
                Some(Consent::Revoked),
                Some(AutoLockPreference::Manual),
            )
            .unwrap();

        let entry = store.registry_entry(&file_id).unwrap();
        assert!(matches!(entry.hydration, Hydration::None));
        assert!(matches!(entry.consent, Consent::Revoked));
        assert!(matches!(entry.auto_lock_preference, AutoLockPreference::Manual));
    }
}
