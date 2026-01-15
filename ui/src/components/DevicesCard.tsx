import { DeviceState } from "../types";
import { StatusBadge } from "./StatusBadge";

interface Props {
  devices: DeviceState[];
  onPromptSync: (deviceId: string) => void;
}

export function DevicesCard({ devices, onPromptSync }: Props) {
  return (
    <div className="card">
      <h3>Devices</h3>
      <div className="devices">
        {devices.map((d) => (
          <div key={d.deviceId} className="device-item">
            <div className="row">
              <div>
                <div style={{ fontWeight: 700 }}>{d.deviceId}</div>
                <div className="muted">Last seen {d.lastSeen}</div>
                {d.lastError && <div className="muted">Error: {d.lastError}</div>}
              </div>
              <StatusBadge state={d.state} />
            </div>
            <div style={{ marginTop: 8 }}>
              <button className="button secondary" onClick={() => onPromptSync(d.deviceId)}>
                Prompt sync
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
