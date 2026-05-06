import React, { useState, useCallback, useRef } from 'react';
import { Upload, Folder } from '../icons/app-icons';
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
  const [filePath, setFilePath] = useState('');
  const [error, setError] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const reset = () => {
    setFilePath('');
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
        JSON.parse(json); // validate
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

  const handleBrowse = useCallback(async () => {
    const selected = await window.electron.selectFileOrDirectory();
    if (!selected) return;
    setFilePath(selected);
    setError('');
  }, []);

  const handlePathImport = useCallback(async () => {
    const trimmed = filePath.trim();
    if (!trimmed) {
      setError('Please enter a file path.');
      return;
    }
    setError('');
    setIsSubmitting(true);
    try {
      const result = await window.electron.readFile(trimmed);
      if (!result || !result.found) {
        throw new Error('Could not read file.');
      }
      const json = result.file as string;
      JSON.parse(json); // validate
      await onImport(json);
      reset();
      onClose();
    } catch (e) {
      setError(e instanceof SyntaxError ? 'Invalid JSON file.' : String(e));
      setIsSubmitting(false);
    }
  }, [filePath, onImport, onClose]);

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && handleClose()}>
      <DialogContent className="sm:max-w-[480px]">
        <DialogHeader>
          <DialogTitle>Import Session</DialogTitle>
          <DialogDescription>
            Drag and drop a session JSON file, or enter its path below.
          </DialogDescription>
        </DialogHeader>

        {/* Drag-and-drop zone */}
        <div
          onDragOver={(e) => {
            e.preventDefault();
            setIsDragging(true);
          }}
          onDragLeave={() => setIsDragging(false)}
          onDrop={handleDrop}
          onClick={() => fileInputRef.current?.click()}
          className={`flex flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed py-8 cursor-pointer transition-colors duration-150 ${
            isDragging
              ? 'border-[#cf6d47] bg-[#cf6d47]/5'
              : 'border-border-subtle bg-background-muted hover:border-border-strong hover:bg-background-medium'
          }`}
        >
          <Upload className="w-7 h-7 text-text-muted" />
          <p className="text-sm text-text-default font-medium">Drop a JSON file here</p>
          <p className="text-xs text-text-muted">or click to browse</p>
          <input
            ref={fileInputRef}
            type="file"
            accept=".json,application/json"
            onChange={handleFileInputChange}
            className="hidden"
          />
        </div>

        {/* Divider */}
        <div className="flex items-center gap-3">
          <div className="flex-1 h-px bg-border-subtle" />
          <span className="text-xs text-text-muted uppercase tracking-wider">or</span>
          <div className="flex-1 h-px bg-border-subtle" />
        </div>

        {/* File path input */}
        <div className="flex gap-2">
          <div className={`flex flex-1 items-center rounded-lg border bg-background-default transition-colors duration-150 focus-within:border-border-strong ${
            error ? 'border-red-500 dark:border-red-400' : 'border-border-subtle'
          }`}>
            <input
              type="text"
              value={filePath}
              onChange={(e) => {
                setFilePath(e.target.value);
                setError('');
              }}
              onKeyDown={(e) => e.key === 'Enter' && !isSubmitting && handlePathImport()}
              placeholder="/path/to/session.json"
              className="flex-1 px-3 py-2 text-sm bg-transparent text-text-default placeholder:text-text-muted focus:outline-none"
            />
            <button
              type="button"
              onClick={handleBrowse}
              disabled={isSubmitting}
              title="Browse for file"
              className="px-2.5 py-2 text-text-muted hover:text-text-default disabled:opacity-50 transition-colors duration-150"
            >
              <Folder className="w-4 h-4" />
            </button>
          </div>
          <Button
            type="button"
            variant="outline"
            onClick={handlePathImport}
            disabled={isSubmitting || !filePath.trim()}
          >
            Load
          </Button>
        </div>

        {error && (
          <p className="text-sm text-red-500 dark:text-red-400 -mt-1">{error}</p>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={handleClose} disabled={isSubmitting}>
            Cancel
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
