import { useCallback, useRef, useState } from 'react';
import { FileStack, FolderTree, Upload } from '../../icons/app-icons';
import { Badge } from '../../ui/badge';
import { Button } from '../../ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '../../ui/dialog';
import type { StagedFileCandidate } from './fileValidation';

interface Props {
  onFiles: (files: StagedFileCandidate[]) => void | Promise<void>;
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

export function Dropzone({ onFiles, onPathPickRequested }: Props) {
  const [dragging, setDragging] = useState(false);
  const [chooserOpen, setChooserOpen] = useState(false);
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
    [onFiles]
  );

  function openChooser() {
    setChooserOpen(true);
  }

  return (
    <>
      <div
        role="button"
        tabIndex={0}
        onClick={openChooser}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            openChooser();
          }
        }}
        onDragEnter={(e) => {
          e.preventDefault();
          dragCounterRef.current++;
          setDragging(true);
        }}
        onDragOver={(e) => {
          e.preventDefault();
        }}
        onDragLeave={(e) => {
          e.preventDefault();
          dragCounterRef.current--;
          if (dragCounterRef.current === 0) setDragging(false);
        }}
        onDrop={onDrop}
        className={`relative cursor-pointer rounded-container border px-4 py-5 text-center transition-colors ${dragging ? 'border-border-strong bg-background-medium' : 'border-border-subtle bg-background-muted tint-interactive'}`}
      >
        <input
          data-testid="knowledge-ingest-file-input"
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
        <div
          className={`mx-auto flex h-10 w-10 items-center justify-center rounded-full transition-colors ${dragging ? 'bg-background-strong text-text-default' : 'bg-background-medium text-text-muted'}`}
        >
          <Upload className="h-5 w-5" />
        </div>
        <div className="mt-2 text-label">Drag and drop to stage</div>
        <div className="mt-1 text-supporting text-text-muted">
          Drop readable files directly, or click to choose files, folders, and archives for backend
          staging.
        </div>
        <div className="mt-3 flex flex-wrap justify-center gap-1.5">
          {[
            '.pdf',
            '.pptx',
            '.xlsx',
            '.docx',
            '.csv',
            '.md',
            '.html',
            '.txt',
            'folders',
            'archives',
          ].map((label) => (
            <Badge key={label} className="font-mono">
              {label}
            </Badge>
          ))}
        </div>
        <div className="mt-2 text-supporting text-text-muted">
          Readable contents are staged file by file. Binaries are skipped, and{' '}
          <span className="font-medium text-text-default">Import from .brkb</span> stays in the
          knowledge base menu for full knowledge-base archives.
        </div>
      </div>

      <Dialog open={chooserOpen} onOpenChange={setChooserOpen}>
        <DialogContent className="sm:max-w-[560px]">
          <DialogHeader>
            <DialogTitle>Stage knowledge sources</DialogTitle>
            <DialogDescription>
              Choose how you want to bring material into the staging queue. Folders and archives are
              unpacked in the backend so their readable contents can be staged separately.
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-3 sm:grid-cols-2">
            <button
              data-testid="knowledge-ingest-browse-files"
              type="button"
              onClick={() => {
                setChooserOpen(false);
                inputRef.current?.click();
              }}
              className="biorouter-modal-row rounded-element px-4 py-4 text-left transition-colors tint-interactive"
            >
              <FileStack className="h-5 w-5 text-text-muted" />
              <div className="mt-3 text-label">Choose files</div>
              <p className="mt-1 text-supporting text-text-muted">
                Stage PDFs, Markdown, HTML, DOCX, CSV, plain text, and similar readable files.
              </p>
            </button>

            <button
              data-testid="knowledge-ingest-browse-path"
              type="button"
              onClick={() => {
                setChooserOpen(false);
                void onPathPickRequested();
              }}
              className="biorouter-modal-row rounded-element px-4 py-4 text-left transition-colors tint-interactive"
            >
              <FolderTree className="h-5 w-5 text-text-muted" />
              <div className="mt-3 text-label">Choose folder or archive</div>
              <p className="mt-1 text-supporting text-text-muted">
                Let Biorouter unpack archives, skip binaries, and stage readable children one by one
                for curation.
              </p>
            </button>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setChooserOpen(false)}>
              Close
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
