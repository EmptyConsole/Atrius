import { VersionEntry } from "../types";

interface Props {
  versions: VersionEntry[];
  onRollback: (id: string) => void;
}

export function VersionsCard({ versions, onRollback }: Props) {
  return (
    <div className="card">
      <h3>Versions</h3>
      <div className="versions">
        {versions.map((v) => (
          <div key={v.id} className="version-item">
            <div className="row">
              <div>
                <div style={{ fontWeight: 700 }}>{v.label}</div>
                <div className="muted">
                  {v.timestamp} • {v.originDevice} • {v.size}
                </div>
                <div className="muted">Hash: {v.hash}</div>
              </div>
              <button className="button secondary" onClick={() => onRollback(v.id)}>
                Rollback
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
