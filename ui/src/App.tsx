import { useState } from "react";
import { FileList } from "./components/FileList";
import { FileDetailView } from "./components/FileDetail";
import { useFiles } from "./hooks/useFiles";

export default function App() {
  const { files, selected, select } = useFiles();
  const [autoLock, setAutoLock] = useState(true);

  return (
    <div className="app-shell">
      <FileList
        files={files}
        selectedId={selected?.fileId}
        onSelect={select}
        onAdd={() => alert("Add file: wire to picker + registry bind")}
        onRemove={(id) => alert(`Remove file ${id}: revoke consent + unbind paths`)}
      />
      {selected ? (
        <FileDetailView
          file={selected}
          onLock={() => alert("Lock: call into core acquire_lock")}
          onUnlock={() => alert("Unlock: call into core release_lock")}
          onToggleAutoLock={() => setAutoLock((v) => !v)}
          autoLockEnabled={autoLock}
          onRollback={(vid) => alert(`Rollback to ${vid}: invoke rollback_to_version`)}
          onPromptSync={(d) => alert(`Prompt sync on ${d}: enqueue transfer`)}
        />
      ) : (
        <div className="detail">
          <div className="muted">Select a file to view details.</div>
        </div>
      )}
    </div>
  );
}
