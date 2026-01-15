import { useEffect, useState } from "react";
import { DeviceState, FileDetail, FileRow, VersionEntry } from "../types";

// Placeholder data source. Replace these with bindings into the Rust core.

const demoVersions: VersionEntry[] = [
  {
    id: "v3",
    label: "v3 (head)",
    timestamp: "2026-01-10 14:22",
    originDevice: "MacBook",
    size: "24 MB",
    hash: "sha256:abcd…1234",
  },
  {
    id: "v2",
    label: "v2",
    timestamp: "2026-01-09 09:11",
    originDevice: "iPad",
    size: "22 MB",
    hash: "sha256:ef56…7890",
  },
  {
    id: "v1",
    label: "v1",
    timestamp: "2026-01-08 18:40",
    originDevice: "MacBook",
    size: "20 MB",
    hash: "sha256:1111…2222",
  },
];

const demoDevices: DeviceState[] = [
  { deviceId: "MacBook", state: "ready", lastSeen: "now" },
  { deviceId: "iPad", state: "syncing", lastSeen: "2m ago" },
  { deviceId: "Windows", state: "available_remote", lastSeen: "1h ago" },
];

const demoFiles: FileDetail[] = [
  {
    fileId: "file-1",
    name: "forest-texture.png",
    state: "syncing",
    lock: { kind: "locked", ownerDevice: "MacBook", ownerUser: "alice" },
    progress: 0.42,
    headVersion: "v3",
    deviceCount: 3,
    originDevice: "MacBook",
    size: "24 MB",
    updatedAt: "2026-01-10 14:22",
    versions: demoVersions,
    devices: demoDevices,
    path: "/Users/alice/Art/forest-texture.png",
  },
  {
    fileId: "file-2",
    name: "ambience.wav",
    state: "ready",
    lock: { kind: "unlocked" },
    headVersion: "v5",
    deviceCount: 2,
    originDevice: "Windows",
    size: "88 MB",
    updatedAt: "2026-01-09 16:05",
    versions: demoVersions,
    devices: demoDevices,
    path: "C:\\Projects\\audio\\ambience.wav",
  },
];

export function useFiles() {
  const [files, setFiles] = useState<FileRow[]>([]);
  const [selected, setSelected] = useState<FileDetail | null>(null);

  useEffect(() => {
    // Replace with real data fetch from the core.
    setFiles(
      demoFiles.map(
        ({ fileId, name, state, lock, progress, headVersion, deviceCount }) => ({
          fileId,
          name,
          state,
          lock,
          progress,
          headVersion,
          deviceCount,
        })
      )
    );
    setSelected(demoFiles[0]);
  }, []);

  const select = (fileId: string) => {
    const detail = demoFiles.find((f) => f.fileId === fileId) || null;
    setSelected(detail);
  };

  return { files, selected, select };
}
