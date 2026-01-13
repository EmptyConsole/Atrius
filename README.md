# Atrius (MVP scaffold)

Atrius is a file-centric, real-time sync tool aimed at game assets. This repo contains an early proof-of-concept: a lightweight relay and a local agent that can watch specific files, push changes in near real time, and keep per-file metadata (locks, versions, device list).

## High-level goals

- File-level sync (no repo/folder requirement)
- Near real-time propagation with resumable chunks
- Binary-friendly (streams, no line diffs)
- Device awareness (who has a file, who is online)
- Basic locking and lightweight version snapshots

## What is included right now

- A TypeScript relay (WebSocket) to coordinate devices and fan out file updates.
- A TypeScript agent that:
  - Registers the device with the relay
  - Watches specific files
  - Sends changes as chunks
  - Receives updates from other devices and writes them locally
  - Stores per-file snapshots before applying remote changes
- Simple in-memory state for the relay (devices, files, locks, versions)

## What is **not** included yet

- Production-ready security (TLS, authentication tokens, key exchange)
- Mobile-native agents (iOS/iPadOS) — this PoC targets desktop first
- Auto-launch/background installers
- Advanced conflict resolution beyond lock + last-writer-wins broadcast
- Persistent relay storage

## Quick start (local demo)

1. Install deps

```
npm install
```

2. Start the relay

```
npm run relay
```

3. In another shell, start an agent for a file

```
npm run agent -- --file /path/to/asset.png --device "Mac-Design"
```

4. On a second machine (or another shell simulating another device), run the agent pointing to the same relay URL and the same file path. Edits on either side should sync in seconds.

## Architecture (current state)

- Transport: WebSockets (`ws`). Messages are JSON envelopes with a `type` and `payload`.
- Relay:
  - Tracks devices (id, name, connection)
  - Tracks per-file membership and lock ownership
  - Broadcasts file chunk streams to subscribers other than the sender
- Agent:
  - Maintains a local registry in `.atrius/registry.json`
  - Watches files with `chokidar`
  - On change: optionally acquires a lock, streams the file in chunks, marks a new version locally, and notifies relay
  - On incoming update: saves a snapshot then writes the new contents

## Roadmap next

- Add auth + encryption (mTLS or Noise handshake)
- Resume/retry with content-addressed chunks and checksums
- Persist relay metadata (SQLite) and per-device tokens
- Native wrappers: Tauri desktop shell, Swift/SwiftUI agent for iOS/iPadOS
- UI: File list, status, locks, versions, device presence

## Notes

This scaffold is deliberately small to validate the file-level sync model quickly. It is not hardened; treat it as a starting point to iterate toward the MVP.\*\*\*
