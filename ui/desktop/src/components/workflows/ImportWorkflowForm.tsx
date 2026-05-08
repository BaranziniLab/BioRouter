import { useState, useCallback, useRef } from 'react';
import { Upload, Download } from '../icons/app-icons';
import { Button } from '../ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from '../ui/dialog';
import { toastSuccess } from '../../toasts';
import { saveWorkflow } from '../../workflow/workflow_management';
import { parseWorkflow } from '../../api';
import type { Workflow } from '../../workflow';

interface ImportWorkflowFormProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

async function parseWorkflowFromFile(fileContent: string): Promise<Workflow> {
  try {
    const response = await parseWorkflow({
      body: { content: fileContent },
      throwOnError: true,
    });
    return response.data.workflow;
  } catch (error) {
    const msg =
      typeof error === 'object' && error !== null && 'message' in error
        ? (error as { message: string }).message
        : 'Unknown error';
    throw new Error(msg);
  }
}

export default function ImportWorkflowForm({ isOpen, onClose, onSuccess }: ImportWorkflowFormProps) {
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

  const processContent = useCallback(
    async (content: string, filename: string) => {
      const isYaml = /\.(ya?ml)$/i.test(filename);
      const isJson = /\.json$/i.test(filename);
      if (!isYaml && !isJson) {
        setError('Please provide a YAML or JSON workflow file.');
        return;
      }
      setError('');
      setIsSubmitting(true);
      try {
        const workflow = await parseWorkflowFromFile(content);
        await saveWorkflow(workflow, null);
        reset();
        onClose();
        onSuccess();
        toastSuccess({ title: workflow.title?.trim() || 'Workflow', msg: 'Workflow imported successfully' });
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        setIsSubmitting(false);
      }
    },
    [onClose, onSuccess]
  );

  const processFile = useCallback(
    async (file: File) => {
      const content = await file.text();
      await processContent(content, file.name);
    },
    [processContent]
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
    <Dialog open={isOpen} onOpenChange={(open) => !open && handleClose()}>
      <DialogContent className="sm:max-w-[480px]">
        <DialogHeader>
          <DialogTitle>Import Workflow</DialogTitle>
          <DialogDescription>
            Drag and drop a workflow YAML or JSON file, or click to browse.
          </DialogDescription>
        </DialogHeader>

        <div
          onDragOver={(e) => { e.preventDefault(); setIsDragging(true); }}
          onDragLeave={() => setIsDragging(false)}
          onDrop={handleDrop}
          onClick={() => !isSubmitting && fileInputRef.current?.click()}
          className={[
            'flex flex-col items-center justify-center gap-2 rounded-xl border-2 border-dashed py-10 cursor-pointer transition-colors duration-150 select-none',
            isDragging
              ? 'border-[#cf6d47] bg-[#cf6d47]/5'
              : error
              ? 'border-red-400 bg-red-50 dark:bg-red-900/10'
              : 'border-border-subtle bg-background-muted hover:border-border-strong hover:bg-background-medium',
          ].join(' ')}
        >
          <input
            ref={fileInputRef}
            type="file"
            accept=".yaml,.yml,.json"
            onChange={handleFileInputChange}
            className="hidden"
          />
          {isSubmitting ? (
            <p className="text-sm text-text-muted animate-pulse">Importing…</p>
          ) : (
            <>
              <Upload className="w-8 h-8 text-text-muted" />
              <p className="text-sm font-medium text-text-default">Drop a YAML or JSON file here</p>
              <p className="text-xs text-text-muted">or click to browse</p>
            </>
          )}
        </div>

        {error && (
          <p className="text-sm text-red-500 dark:text-red-400">{error}</p>
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

export function ImportWorkflowButton({ onClick }: { onClick: () => void }) {
  return (
    <Button onClick={onClick} variant="outline" className="flex items-center gap-2">
      <Download className="w-4 h-4" />
      Import Workflow
    </Button>
  );
}
