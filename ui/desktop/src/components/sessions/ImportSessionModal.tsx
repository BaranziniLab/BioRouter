import React, { useState, useCallback, useRef } from 'react';
import { Upload } from '../icons/app-icons';
import { Button } from '../ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../ui/dialog';

interface ImportSessionModalProps {
  isOpen: boolean;
  onClose: () => void;
  onImport: (json: string) => Promise<void>;
}

export function ImportSessionModal({ isOpen, onClose, onImport }: ImportSessionModalProps) {
  const [isDragging, setIsDragging] = useState(false);
  const [error, setError] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const reset = () => {
    setError('');
    setIsDragging(false);
    setIsSubmitting(false);
  };

  const handleClose = () => {
    reset();
    onClose();
  };

  const processFile = useCallback(
    async (file: File) => {
      if (!file.name.endsWith('.json') && file.type !== 'application/json') {
        setError('Please provide a JSON file.');
        return;
      }
      setError('');
      setIsSubmitting(true);
      try {
        const json = await file.text();
        JSON.parse(json);
        await onImport(json);
        reset();
        onClose();
      } catch (e) {
        setError(e instanceof SyntaxError ? 'Invalid JSON file.' : String(e));
        setIsSubmitting(false);
      }
    },
    [onImport, onClose]
  );

  const handleDrop = useCallback(
    (e: React.DragEvent<HTMLDivElement>) => {
      e.preventDefault();
      setIsDragging(false);
      const file = e.dataTransfer.files[0];
      if (file) processFile(file);
    },
    [processFile]
  );

  const handleFileInputChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) processFile(file);
      e.target.value = '';
    },
    [processFile]
  );

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && !isSubmitting && handleClose()}>
      <DialogContent dismissible={!isSubmitting} className="sm:max-w-[480px]">
        <DialogHeader>
          <DialogTitle>Import chat</DialogTitle>
          <DialogDescription>Drag and drop a chat JSON file, or click to browse.</DialogDescription>
        </DialogHeader>

        <div
          onDragOver={(e) => {
            e.preventDefault();
            setIsDragging(true);
          }}
          onDragLeave={() => setIsDragging(false)}
          onDrop={handleDrop}
          onClick={() => !isSubmitting && fileInputRef.current?.click()}
          className={[
            'biorouter-modal-panel flex flex-col items-center justify-center gap-2 rounded-container py-10 cursor-pointer transition-colors select-none',
            isDragging
              ? '!border-block-teal bg-block-teal/5'
              : error
                ? '!border-border-danger bg-background-danger/10'
                : 'hover:!border-border-strong tint-interactive',
          ].join(' ')}
        >
          <input
            ref={fileInputRef}
            type="file"
            accept=".json,application/json"
            onChange={handleFileInputChange}
            className="hidden"
          />
          {isSubmitting ? (
            <p className="text-sm text-text-muted animate-pulse">Importing…</p>
          ) : (
            <>
              <Upload className="w-8 h-8 text-text-muted" />
              <p className="text-sm font-medium text-text-default">Drop a JSON file here</p>
              <p className="text-xs text-text-muted">or click to browse</p>
            </>
          )}
        </div>

        {error && <p className="text-sm text-text-danger">{error}</p>}

        <DialogFooter>
          <Button variant="outline" onClick={handleClose} disabled={isSubmitting}>
            Cancel
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
