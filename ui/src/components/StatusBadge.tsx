import { FileState } from "../types";

export function StatusBadge({ state }: { state: FileState }) {
  const label = {
    ready: "Ready",
    syncing: "Syncing",
    lock_blocked: "Blocked",
    conflict: "Conflict",
    error: "Error",
    available_remote: "Remote",
    absent: "Absent",
  }[state];

  const cls = {
    ready: "status-ready",
    syncing: "status-syncing",
    lock_blocked: "status-blocked",
    conflict: "status-conflict",
    error: "status-error",
    available_remote: "status-remote",
    absent: "status-remote",
  }[state];

  return <span className={`badge ${cls}`}>{label}</span>;
}
