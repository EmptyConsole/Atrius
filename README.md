# Atrius

Atrius is a real-time, cross-platform, file-centric sync system for large binary game assets. It treats every file as an independent entity with its own identity, state, history, and lock status—never as part of a folder or repository.

## What this project contains
- Product-aligned data model for the MVP, focused on per-file identity, sync state, locks, and lightweight versioning.
- Guidance on local registry design to keep stable file identity while paths change.
- State and flow descriptions for add, edit, rename/move, conflict, and removal.

## Prerequisites
- Rust toolchain via rustup: `curl https://sh.rustup.rs -sSf | sh`
- Verify: `cargo --version` and `rustc --version`
- Recommended: rust-analyzer in your editor for IDE support.

## Build & test
```bash
cd /Users/shaayeralam/empty-console/Atrius
cargo test          # runs model invariant tests
```

If `cargo` is missing, install Rust with rustup (above), reopen your shell, and rerun.

## Use as a library (path dependency)
In another crate’s `Cargo.toml`:
```toml
atrius = { path = "/Users/shaayeralam/empty-console/Atrius" }
```

Minimal usage:
```rust
use atrius::{
    assert_file_invariants, DeviceFileState, DeviceFileStateKind, EncryptionInfo, FileRecord,
    VersionRecord,
};
use chrono::Utc;
use ulid::Ulid;

fn main() {
    let file_id = Ulid::new();
    let head = Ulid::new();
    let record = FileRecord {
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
            content_hash: "sha256hex".into(),
            size_bytes: 123,
            chunks: vec![],
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
            key_id: "k1".into(),
            algo: "AES-256-GCM".into(),
            iv_salt: None,
        },
    };

    assert_file_invariants(&record).expect("record invariants hold");
}
```

## Dev tips
- `cargo fmt` to format, `cargo clippy` for lints.
- Run tests with logs: `cargo test -- --nocapture`.
- Types and invariants live in `src/model.rs`; `docs/data-model.md` explains the rationale.

## Non-goals (per MVP)
- No Git/repo workflows, branching, or folder-wide auto-sync.
- No engine plugins, marketplaces, or advanced merge tools.
- No silent overwrites; user control and consent are required.

## Start here
Read `docs/data-model.md` for the detailed architecture, invariants, and example data structures that implement the prompt in `firstPrompt.txt`.
