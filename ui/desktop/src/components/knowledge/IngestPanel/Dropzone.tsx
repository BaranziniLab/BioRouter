import { useCallback, useRef, useState } from 'react';
import { Clipboard, FolderOpen, Upload } from 'lucide-react';
import type { StagedFileCandidate } from './fileValidation';

interface Props {
  onFiles: (files: StagedFileCandidate[]) => void | Promise<void>;
  onPasteTextRequested: () => void;
  onPathPickRequested: () => void | Promise<void>;
}

function labelFromPath(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function tryParseLocalPath(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed || trimmed.startsWith('#')) {
    return null;
  }

  if (trimmed.startsWith('file://')) {
    try {
      return decodeURIComponent(new URL(trimmed).pathname);
    } catch {
      return null;
    }
  }

  if (trimmed.startsWith('/') || /^[A-Za-z]:[\\/]/.test(trimmed)) {
    return trimmed;
  }

  return null;
}

function getDroppedPathCandidates(e: React.DragEvent): StagedFileCandidate[] {
  const candidates = new Map<string, StagedFileCandidate>();
  const uriList = e.dataTransfer.getData('text/uri-list');
  const plainText = e.dataTransfer.getData('text/plain');

  for (const raw of `${uriList}\n${plainText}`.split('\n')) {
    const parsed = tryParseLocalPath(raw);
    if (!parsed) {
      continue;
    }
    candidates.set(parsed, { path: parsed, label: labelFromPath(parsed) });
  }

  return [...candidates.values()];
}

export function Dropzone({ onFiles, onPasteTextRequested, onPathPickRequested }: Props) {
  const [dragging, setDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  // Counter-based drag tracking prevents flicker when cursor moves over child elements.
  const dragCounterRef = useRef(0);

  const onDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      dragCounterRef.current = 0;
      setDragging(false);
      const droppedFiles = Array.from(e.dataTransfer.files).map((file) => ({ file }));
      const droppedPaths = getDroppedPathCandidates(e);
      const candidates = [...droppedFiles, ...droppedPaths];
      if (candidates.length > 0) void onFiles(candidates);
    },
    [onFiles],
  );

  return (
    <div
      onDragEnter={(e) => {
        e.preventDefault();
        dragCounterRef.current++;
        setDragging(true);
      }}
      onDragOver={(e) => {
        e.preventDefault();
        // Don't touch dragging state — counter handles it.
      }}
      onDragLeave={(e) => {
        e.preventDefault();
        dragCounterRef.current--;
        if (dragCounterRef.current === 0) setDragging(false);
      }}
      onDrop={onDrop}
      className={`relative border-2 border-dashed rounded-xl p-6 text-center transition-colors ${
        dragging
          ? 'border-success-default bg-background-muted'
          : 'border-border-subtle bg-background-surface'
      }`}
    >
      <input
        ref={inputRef}
        type="file"
        multiple
        className="hidden"
        onChange={(e) => {
          const files = e.target.files ? Array.from(e.target.files) : [];
          if (files.length > 0) {
            void onFiles(files.map((file) => ({ file })));
          }
          e.target.value = '';
        }}
      />
      <Upload className="w-7 h-7 mx-auto text-text-muted" />
      <div className="mt-2 text-sm font-medium">Drag & drop to stage</div>
      <div className="mt-1 text-xs text-text-muted">Papers, notes, HTML pages, and curated datasets</div>
      <div className="mt-3 flex flex-wrap gap-1.5 justify-center text-[10px] font-mono text-text-muted">
        {['pdf', 'md', 'html', 'docx', 'csv', 'txt'].map((ext) => (
          <span key={ext} className="border border-border-subtle rounded px-1.5 py-0.5">
            .{ext}
          </span>
        ))}
      </div>
      <div className="mt-2 text-[11px] text-text-muted">
        Use <span className="font-medium text-text-default">Import from .brkb</span> in the
        knowledge base menu for full archive imports.
      </div>
      <div className="mt-3 flex gap-2">
        <button
          onClick={() => inputRef.current?.click()}
          className="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg border border-border-subtle bg-background-default text-xs hover:bg-background-muted"
        >
          <FolderOpen className="w-3 h-3" /> Browse files
        </button>
        <button
          onClick={() => void onPathPickRequested()}
          className="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg border border-border-subtle bg-background-default text-xs hover:bg-background-muted"
        >
          <FolderOpen className="w-3 h-3" /> Browse folder or archive
        </button>
        <button
          onClick={onPasteTextRequested}
          className="flex-1 inline-flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg border border-border-subtle bg-background-default text-xs hover:bg-background-muted"
        >
          <Clipboard className="w-3 h-3" /> Paste text
        </button>
      </div>
    </div>
  );
}
