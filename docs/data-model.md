# Atrius Core File-Centric Data Model

This document applies the requirements in `firstPrompt.txt` to a concrete MVP data model. The focus is on identities, state, metadata boundaries, invariants, and flows—not production code.

## Product-aligned constraints

- Per-file, path-independent identity; never folder/repo scoped.
- Near real-time propagation across Windows, macOS, iOS, iPadOS.
- Large binary assets first; no silent overwrites.
- Visible version history (lightweight, recent window).
- Explicit user consent and control for every synced file.
- Background/resumable transfer; encrypted transport; authenticated devices.
- Locking (manual and optional auto-on-edit) visible across devices.

## Core identities (stable, path-independent)

- `FileId`: ULID/UUID minted when a user adds a file to Atrius; never derived from path. Persisted locally and shared.
- `DeviceId`: unique per installed client; cryptographically bound to auth keys.
- `VersionId`: monotonic-ish ULID per file version.
- `LockId`: ULID per lock acquisition.
- `TransferSessionId`: ULID per active transfer attempt.

## Shared vs local metadata

| Aspect     | Shared across devices           | Local-only (registry)                           |
| ---------- | ------------------------------- | ----------------------------------------------- |
| Identity   | `FileId`, `VersionId`, `LockId` | `FileId` ↔ `path` mapping, user consent         |
| Sync state | Per-device state vector         | Local hydration state (present/absent), pinning |
| Versions   | Recent window (ordered)         | Cached blobs/chunks, prefetch hints             |
| Locks      | Current lock record             | Local auto-lock preference                      |
| Audit      | Origin device per version       | Local error logs                                |
| Security   | Encryption envelope info        | Local key handles (never remote)                |

## Core entities (conceptual)

- `FileRecord` (shared):
  - `fileId`, `originDeviceId`, `createdAt`
  - `headVersionId`
  - `versions[]` (bounded recent window)
  - `lock` (nullable)
  - `deviceStates[]` (per-device sync vector)
  - `encryption` (algo, key id, salt/iv per version)
- `VersionRecord` (shared):
  - `versionId`, `fileId`, `parentVersionId`
  - `originDeviceId`, `timestamp`
  - `contentHash` (strong, e.g., SHA-256), `sizeBytes`
  - `chunks[]` (offset, length, chunkHash) for resumable transfer
- `LockRecord` (shared):
  - `lockId`, `fileId`, `ownerDeviceId`, `ownerUserId`
  - `mode: exclusive`
  - `acquiredAt`, optional `autoLock: boolean`, optional `expiresAt`
- `DeviceFileState` (shared):
  - `deviceId`
  - `state`: `absent | available_remote | pulling | ready | pushing | lock_blocked | conflict | error`
  - `knownHeadVersionId`, `lastSeenAt`, `lastError?`
- `LocalRegistryEntry` (local):
  - `fileId`
  - `paths[]`: `{ path, lastSeenAt, writable: boolean }` (supports moves/renames)
  - `hydration`: `fully_present | partial | none`
  - `consent`: `approved | revoked`
  - `localVersionId` (what the disk reflects)
  - `pin`: `none | keep_latest`
  - `autoLockPreference`: `on_edit | manual`
- `TransferSession` (local + transient shared status):
  - `transferSessionId`, `fileId`, `direction: push|pull`
  - `fromDeviceId`, `toDeviceId`
  - `activeChunks`, `retryCount`, `status`

## Local registry (persistent, path-stable)

Store in an embedded DB (SQLite) per device; never assumes folder ownership.

- `files(fileId PRIMARY KEY, originDeviceId, createdAt, consent, autoLockPreference)`
- `paths(id PK, fileId FK, path UNIQUE, lastSeenAt, writable)`
- `local_state(fileId FK, localVersionId, hydration, pin, lastError)`
- `versions_cache(versionId PK, fileId FK, sizeBytes, contentHash, storedPath)`
- Invariant: `paths.fileId` refers to exactly one `FileId`; moving/renaming only mutates `path` rows—`fileId` remains.

## State model (per device, per file)

States: `absent` → `available_remote` → (`pulling` → `ready`) or (`pushing` → `ready`); blockers: `lock_blocked`, `conflict`, `error`.

Transitions (non-exhaustive):

- Add/consent: `absent` → `pushing` (new file) or `available_remote` (if remote exists) depending on role.
- Pull: `available_remote` → `pulling` → `ready` (on success) or `error`.
- Push edit: `ready` → `pushing` → `ready`.
- Lock conflict: any → `lock_blocked` while lock held by other; resumes to prior target state after release.
- Version conflict (no lock): on divergent head, enter `conflict`; requires user choice (keep local, take remote, or duplicate as new version) with both payloads preserved.
- Error: retry with backoff; after threshold, surface to user but keep resumable session.

## Versioning and integrity

- Append-only version window (e.g., last N versions or age-based TTL) per file; never rewrite history within the window.
- Each version includes `contentHash` and chunk hashes for integrity; resume by verifying completed chunks only.
- Head advancement rule: only one head at a time; head = latest accepted `VersionId`.
- Divergence handling: if a push targets stale head and no lock, mark `conflict`; do not fast-forward automatically.

## Locking rules

- Locks are advisory but enforced by the engine: a device must acquire an exclusive lock before writing unless user disables auto-lock.
- Auto-lock on edit: before writing to disk, client attempts `LockRecord` acquire; if denied, local state -> `lock_blocked`.
- Manual lock: user-triggered; persists until release or optional `expiresAt`.
- Lock visibility: `LockRecord` propagated with owner and timestamps; shown in UI on all devices.
- Invariant: At most one active lock per `fileId`. A push without the lock is rejected unless policy explicitly allows last-write-wins with explicit user confirmation.

## Conflict handling (no silent overwrite)

- With lock: no conflict; serialized writers.
- Without lock: stale head push → `conflict`; both versions stored. User must resolve by selecting winner or keeping both (creates new `VersionId` for local).
- Resolution always preserves losing payload as a version; never discard data silently.

## Flows (high level)

- **Add file on Device A**: generate `FileId`; registry records path; upload initial version → shared `FileRecord`; Device A state `ready`.
- **Connect Device B**: receives metadata; sees `available_remote`; on consent, pulls to chosen path (registry maps `FileId` to chosen path) → `ready`.
- **Rename/move locally**: update `paths` row only; `FileId` unchanged; sync unaffected.
- **Edit with auto-lock**: acquire lock → write → push new `VersionId` → release lock (if auto) → other devices pull.
- **Offline edit**: cached lock attempt fails; mark "pending lock"; upon reconnection, acquire lock then push; if conflicting head appeared, enter `conflict`.
- **Remove from sync (local)**: set `consent=revoked`, `hydration=none`, purge cached blobs; shared metadata untouched; other devices unaffected.

## Security and consent

- Device authentication required before exchanging metadata.
- Transfers encrypted (e.g., TLS); per-file encryption envelope includes key id, IV/salt; actual key material stored locally or via secure enclave; never transmitted in plaintext.
- No file content leaves a device without explicit consent (add/approve action).

## Minimal shape sketch (TypeScript-ish)

```ts
type State =
  | "absent"
  | "available_remote"
  | "pulling"
  | "ready"
  | "pushing"
  | "lock_blocked"
  | "conflict"
  | "error";

interface FileRecord {
  fileId: string;
  originDeviceId: string;
  headVersionId: string;
  versions: VersionRecord[];
  lock?: LockRecord;
  deviceStates: DeviceFileState[];
  encryption: { keyId: string; algo: "AES-256-GCM" };
}

interface LocalRegistryEntry {
  fileId: string;
  paths: { path: string; lastSeenAt: number; writable: boolean }[];
  localVersionId: string | null;
  hydration: "fully_present" | "partial" | "none";
  consent: "approved" | "revoked";
  autoLockPreference: "on_edit" | "manual";
}
```

## Invariants checklist

- `FileId` never derives from path; moving/renaming only updates registry paths.
- At most one active lock per file; lock owner visible on all devices.
- Head changes only by accepting a `VersionId`; all pushes reference the head they were based on.
- No push overwrites unacknowledged remote edits; conflicts surface explicitly with both payloads retained.
- Local registry persists across restarts; absence of a path does not delete the file’s identity or metadata.
