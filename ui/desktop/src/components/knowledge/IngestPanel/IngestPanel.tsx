import { useState } from 'react';
import { useStagedSources } from '../hooks/useStagedSources';
import { Dropzone } from './Dropzone';

function genId(): string {
  return Date.now().toString(36) + Math.random().toString(36).substring(2, 9);
}

export function IngestPanel() {
  const { items, add, remove, update, clear } = useStagedSources();
  const [showPasteBox, setShowPasteBox] = useState(false);

  function onFiles(files: File[]) {
    for (const file of files) {
      add({ kind: 'file', id: genId(), file, status: 'pending' });
    }
  }

  // Suppress unused-var lint until later tasks use these
  void remove;
  void update;
  void clear;

  return (
    <div className="flex flex-col gap-4 p-4">
      <Dropzone onFiles={onFiles} onPasteTextRequested={() => setShowPasteBox(true)} />
      {/* PasteTextBox + StagedList + Digest button come in Tasks 6-8 */}
      {showPasteBox && (
        <div className="text-xs text-text-muted">Paste box coming in Task 6…</div>
      )}
      <div className="text-xs text-text-muted">Staged: {items.length}</div>
    </div>
  );
}
