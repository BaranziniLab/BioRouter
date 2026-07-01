import { useCallback, useRef, useState } from 'react';
import { FileStack, FolderTree, Upload } from 'lucide-react';
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
        className={`relative cursor-pointer rounded-xl px-4 py-5 text-center transition-all focus:outline-none focus:ring-1 focus:ring-ring ${
          dragging
            ? 'bg-block-teal/10 shadow-[inset_0_0_0_1px_color-mix(in_srgb,var(--block-teal)_38%,transparent)]'
            : 'hover:bg-background-default/70'
        }`}
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
        <Upload className="mx-auto h-7 w-7 text-text-muted" />
        <div className="mt-2 text-sm font-medium">Drag and drop to stage</div>
        <div className="mt-1 text-xs leading-5 text-text-muted">
          Drop readable files directly, or click to choose files, folders, and archives for backend
          staging.
        </div>
        <div className="mt-3 flex flex-wrap justify-center gap-1.5 text-[11px] font-mono text-text-muted">
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
            <span key={label} className="rounded-md bg-background-default/55 px-1.5 py-0.5">
              {label}
            </span>
          ))}
        </div>
        <div className="mt-2 text-[11px] leading-5 text-text-muted">
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
              className="biorouter-modal-row rounded-2xl px-4 py-4 text-left transition-colors hover:!border-border-default hover:bg-background-default"
            >
              <FileStack className="h-5 w-5 text-text-muted" />
              <div className="mt-3 text-sm font-medium">Choose files</div>
              <p className="mt-1 text-xs leading-5 text-text-muted">
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
              className="biorouter-modal-row rounded-2xl px-4 py-4 text-left transition-colors hover:!border-border-default hover:bg-background-default"
            >
              <FolderTree className="h-5 w-5 text-text-muted" />
              <div className="mt-3 text-sm font-medium">Choose folder or archive</div>
              <p className="mt-1 text-xs leading-5 text-text-muted">
                Let BioRouter unpack archives, skip binaries, and stage readable children one by one
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
