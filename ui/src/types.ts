export type FileState =
  | "ready"
  | "syncing"
  | "lock_blocked"
  | "conflict"
  | "error"
  | "available_remote"
  | "absent";

export type LockState =
  | { kind: "unlocked" }
  | { kind: "locked"; ownerDevice: string; ownerUser: string };

export interface VersionEntry {
  id: string;
  label: string;
  timestamp: string;
  originDevice: string;
  size: string;
  hash: string;
}

export interface DeviceState {
  deviceId: string;
  state: FileState;
  lastSeen: string;
  lastError?: string;
}

export interface FileRow {
  fileId: string;
  name: string;
  state: FileState;
  lock: LockState;
  progress?: number;
  headVersion: string;
  deviceCount: number;
}

export interface FileDetail extends FileRow {
  path?: string;
  originDevice: string;
  size: string;
  updatedAt: string;
  lock: LockState;
  versions: VersionEntry[];
  devices: DeviceState[];
  lastError?: string;
}
