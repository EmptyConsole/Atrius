import { FileDetail } from "../types";
import { StatusBadge } from "./StatusBadge";
import { LockCard } from "./LockCard";
import { VersionsCard } from "./VersionsCard";
import { DevicesCard } from "./DevicesCard";

interface Props {
  file: FileDetail;
  onLock: () => void;
  onUnlock: () => void;
  onToggleAutoLock: () => void;
  autoLockEnabled: boolean;
  onRollback: (versionId: string) => void;
  onPromptSync: (deviceId: string) => void;
}

export function FileDetailView({
  file,
  onLock,
  onUnlock,
  onToggleAutoLock,
  autoLockEnabled,
  onRollback,
  onPromptSync,
}: Props) {
  return (
    <div className="detail">
      <div className="card">
        <div className="row">
          <div>
            <div style={{ fontSize: 20, fontWeight: 700 }}>{file.name}</div>
            <div className="muted">
              {file.size} • Updated {file.updatedAt} • Origin {file.originDevice}
            </div>
            {file.path && <div className="muted">Path: {file.path}</div>}
          </div>
          <StatusBadge state={file.state} />
        </div>
        {file.lastError && <div className="muted" style={{ marginTop: 6 }}>Error: {file.lastError}</div>}
      </div>

      <LockCard
        lock={file.lock}
        onLock={onLock}
        onUnlock={onUnlock}
        autoLockEnabled={autoLockEnabled}
        onToggleAutoLock={onToggleAutoLock}
      />

      <VersionsCard versions={file.versions} onRollback={onRollback} />

      <DevicesCard devices={file.devices} onPromptSync={onPromptSync} />
    </div>
  );
}
