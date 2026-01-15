use std::collections::HashSet;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    ChunkRef, DeviceId, FileId, TransferDirection, TransferSession, TransferSessionId,
    TransferStatus, VersionId,
};

/// Plan of chunks to send or fetch. Derived from a VersionRecord's chunk list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferPlan {
    pub file_id: FileId,
    pub version_id: VersionId,
    pub direction: TransferDirection,
    pub chunks: Vec<ChunkRef>,
}

/// Tracks in-flight or completed chunks for resumable transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferProgress {
    pub session_id: TransferSessionId,
    pub started_at: SystemTime,
    pub completed_chunks: HashSet<u64>, // keyed by chunk offset
    pub failed_chunks: HashSet<u64>,    // for retry bookkeeping
}

/// Retry policy for interrupted or failed chunks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff: Duration,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransferError {
    #[error("chunk not found in plan at offset {0}")]
    ChunkMissing(u64),
    #[error("max retries exceeded for chunk at offset {0}")]
    MaxRetries(u64),
    #[error("transfer already completed")]
    Completed,
}

impl TransferProgress {
    pub fn new(session_id: TransferSessionId) -> Self {
        Self {
            session_id,
            started_at: SystemTime::now(),
            completed_chunks: HashSet::new(),
            failed_chunks: HashSet::new(),
        }
    }

    /// Mark a chunk as done. Idempotent.
    pub fn mark_done(&mut self, offset: u64) {
        self.completed_chunks.insert(offset);
        self.failed_chunks.remove(&offset);
    }

    /// Mark a chunk failure for retry tracking.
    pub fn mark_failed(&mut self, offset: u64) {
        if !self.completed_chunks.contains(&offset) {
            self.failed_chunks.insert(offset);
        }
    }

    pub fn is_complete(&self, plan: &TransferPlan) -> bool {
        plan.chunks
            .iter()
            .all(|c| self.completed_chunks.contains(&c.offset))
    }
}

/// Compute the next chunk to send/fetch, skipping completed items.
pub fn next_chunk(plan: &TransferPlan, progress: &TransferProgress) -> Option<ChunkRef> {
    plan.chunks
        .iter()
        .find(|c| !progress.completed_chunks.contains(&c.offset))
        .cloned()
}

/// Decide if a chunk can be retried under the policy.
pub fn can_retry(
    offset: u64,
    attempt: u32,
    policy: &RetryPolicy,
) -> Result<(), TransferError> {
    if attempt >= policy.max_attempts {
        return Err(TransferError::MaxRetries(offset));
    }
    Ok(())
}

/// Create a TransferSession view from a plan/progress/status.
pub fn to_session(
    plan: &TransferPlan,
    progress: &TransferProgress,
    from: DeviceId,
    to: DeviceId,
    status: TransferStatus,
) -> TransferSession {
    TransferSession {
        transfer_session_id: progress.session_id,
        file_id: plan.file_id,
        direction: plan.direction.clone(),
        from_device_id: from,
        to_device_id: to,
        active_chunks: plan.chunks.clone(),
        retry_count: progress.failed_chunks.len() as u32,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ulid() -> FileId {
        ulid::Ulid::new()
    }

    fn plan() -> TransferPlan {
        TransferPlan {
            file_id: ulid(),
            version_id: ulid(),
            direction: TransferDirection::Push,
            chunks: vec![
                ChunkRef {
                    offset: 0,
                    length: 10,
                    hash: "h0".into(),
                },
                ChunkRef {
                    offset: 10,
                    length: 10,
                    hash: "h1".into(),
                },
            ],
        }
    }

    #[test]
    fn progresses_through_chunks() {
        let plan = plan();
        let mut progress = TransferProgress::new(ulid());
        let c1 = next_chunk(&plan, &progress).unwrap();
        assert_eq!(c1.offset, 0);
        progress.mark_done(c1.offset);
        let c2 = next_chunk(&plan, &progress).unwrap();
        assert_eq!(c2.offset, 10);
        progress.mark_done(c2.offset);
        assert!(next_chunk(&plan, &progress).is_none());
        assert!(progress.is_complete(&plan));
    }

    #[test]
    fn retry_limits() {
        let policy = RetryPolicy {
            max_attempts: 3,
            backoff: Duration::from_secs(1),
        };
        assert!(can_retry(0, 0, &policy).is_ok());
        assert!(can_retry(0, 2, &policy).is_ok());
        assert!(can_retry(0, 3, &policy).is_err());
    }

    #[test]
    fn session_view_is_composed() {
        let plan = plan();
        let progress = TransferProgress::new(ulid());
        let session = to_session(
            &plan,
            &progress,
            ulid(),
            ulid(),
            TransferStatus::InProgress,
        );
        assert_eq!(session.file_id, plan.file_id);
        assert_eq!(session.active_chunks.len(), 2);
    }
}
