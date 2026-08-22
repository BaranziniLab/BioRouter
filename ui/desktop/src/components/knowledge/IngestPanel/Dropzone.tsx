import { useCallback, useRef, useState } from 'react';
import { FileStack, FolderTree, Info, Upload } from '../../icons/app-icons';
import { Button } from '../../ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '../../ui/Tooltip';
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
        className={`relative cursor-pointer rounded-container border px-4 py-4 text-center transition-colors ${dragging ? 'border-border-strong bg-background-medium' : 'border-border-subtle bg-background-muted tint-interactive'}`}
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
        {/* The medallion is `EmptyState`'s own plate: a 48px `rounded-container`
            box with a 24px icon (`empty-state.tsx:34-35`). Not `rounded-full` —
            A-04 restricts that to status dots, the switch knob and avatars, and
            a 48px circle is none of them. The 24px icon follows what SHIPS: the
            primitive draws `h-6 w-6` while `--icon-banner` says 20px, and that
            disagreement is a design-system item rather than something this
            surface silently picks a side of. */}
        {/* ⚠ **330px → ~132px** (R-06), and every pixel of it came off
            decoration rather than function. Measured in Chrome at the rail's
            true 300px width, the dropzone was 330px tall of which 260px was a
            48px medallion, two paragraphs totalling 251 characters wrapping to
            three and four lines, and a ten-chip extension row wrapping to three
            rows — in a rail whose resting content already ran 899px inside a
            745px viewport. What survives is the affordance; what moved behind
            the ⓘ is the reference material, which a user needs once and then
            never again. */}
        <div
          className={`mx-auto flex h-8 w-8 items-center justify-center rounded-element border border-border-subtle transition-colors ${dragging ? 'bg-background-strong text-text-default' : 'bg-background-muted text-text-muted'}`}
        >
          <Upload className="h-4 w-4" aria-hidden="true" />
        </div>
        <div className="mt-2 text-label">Drag and drop to stage</div>
        <div className="mt-1 flex items-center justify-center gap-1 text-supporting text-text-muted">
          <span>PDF, Office, web pages, folders</span>
          <Tooltip>
            <TooltipTrigger asChild>
              <span
                tabIndex={0}
                role="button"
                aria-label="Which files can be staged"
                className="biorouter-focus-surface rounded-inner px-0.5 text-text-subtle"
                onClick={(e) => e.stopPropagation()}
                onKeyDown={(e) => e.stopPropagation()}
              >
                <Info className="h-icon-row w-icon-row" aria-hidden="true" />
              </span>
            </TooltipTrigger>
            <TooltipContent className="max-w-[17rem]">
              Readable files are staged one by one: .pdf, .pptx, .xlsx, .docx, .csv, .md, .html,
              .txt, plus folders and archives, which are unpacked in the backend. Binaries are
              skipped. <span className="text-text-default">Import from .brkb</span> stays in the
              knowledge-base menu for full knowledge-base archives.
            </TooltipContent>
          </Tooltip>
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
            <Button type="button" variant="secondary" onClick={() => setChooserOpen(false)}>
              Close
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
