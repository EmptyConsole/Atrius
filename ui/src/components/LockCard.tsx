import { LockState } from "../types";

interface Props {
  lock: LockState;
  onLock: () => void;
  onUnlock: () => void;
  autoLockEnabled: boolean;
  onToggleAutoLock: () => void;
}

export function LockCard({ lock, onLock, onUnlock, autoLockEnabled, onToggleAutoLock }: Props) {
  return (
    <div className="card">
      <div className="row">
        <h3>Lock</h3>
        <label style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <input type="checkbox" checked={autoLockEnabled} onChange={onToggleAutoLock} />
          Auto-lock on edit
        </label>
      </div>
      <div style={{ marginBottom: 8 }}>
        {lock.kind === "locked" ? (
          <div className="muted">Locked by {lock.ownerDevice} ({lock.ownerUser})</div>
        ) : (
          <div className="muted">Unlocked</div>
        )}
      </div>
      <div style={{ display: "flex", gap: 8 }}>
        <button className="button" onClick={onLock}>Lock</button>
        <button className="button secondary" onClick={onUnlock}>Unlock</button>
      </div>
    </div>
  );
}
