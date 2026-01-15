import { FileRow } from "../types";
import { StatusBadge } from "./StatusBadge";

interface Props {
  files: FileRow[];
  selectedId?: string;
  onSelect: (id: string) => void;
  onAdd: () => void;
  onRemove: (id: string) => void;
}

export function FileList({ files, selectedId, onSelect, onAdd, onRemove }: Props) {
  return (
    <div className="pane">
      <div className="header">
        <button className="button" onClick={onAdd}>
          Add file
        </button>
        <button
          className="button secondary"
          onClick={() => selectedId && onRemove(selectedId)}
          disabled={!selectedId}
        >
          Remove
        </button>
      </div>
      <div className="list">
        {files.map((f) => (
          <div
            key={f.fileId}
            className={`list-item ${selectedId === f.fileId ? "active" : ""}`}
            onClick={() => onSelect(f.fileId)}
          >
            <div className="row">
              <div>
                <div style={{ fontWeight: 700 }}>{f.name}</div>
                <div className="muted">
                  Head {f.headVersion} • Devices {f.deviceCount}
                </div>
              </div>
              <StatusBadge state={f.state} />
            </div>
            <div className="muted" style={{ marginTop: 6 }}>
              {f.lock.kind === "locked"
                ? `Locked by ${f.lock.ownerDevice}`
                : "Unlocked"}
            </div>
            {typeof f.progress === "number" && (
              <div className="muted" style={{ marginTop: 4 }}>
                Progress: {(f.progress * 100).toFixed(0)}%
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
