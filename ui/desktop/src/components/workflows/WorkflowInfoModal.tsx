import React, { useEffect, useRef, useState } from 'react';
import { Button } from '../ui/button';

interface WorkflowInfoModalProps {
  infoLabel?: string;
  originalValue?: string;
  isOpen: boolean;
  onClose: () => void;
  onSaveValue?: (val: string) => void;
}
export default function WorkflowInfoModal({
  infoLabel = '',
  isOpen,
  onClose,
  originalValue = '',
  onSaveValue = () => {},
}: WorkflowInfoModalProps) {
  const [value, setValue] = useState(originalValue);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    if (isOpen) {
      setValue(originalValue);
      textareaRef.current?.focus();
    }
  }, [isOpen, originalValue]);

  const onSave = (event: React.FormEvent) => {
    onSaveValue(value);
    event.preventDefault();
    onClose();
  };
  if (!isOpen) return null;
  return (
    <div className="biorouter-modal-overlay fixed inset-0 transition-colors animate-[fadein_200ms_ease-in_forwards] z-[1000]">
      <div className="biorouter-modal-surface fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 flex flex-col min-w-[80%] min-h-[80%] bg-background-default overflow-hidden px-8 pt-[24px] pb-0">
        <div className="flex mb-6">
          <h2 className="text-base font-semibold text-text-default">Edit {infoLabel}</h2>
        </div>
        <div className="flex flex-col flex-grow overflow-y-auto space-y-8">
          <textarea
            ref={textareaRef}
            className="biorouter-modal-panel w-full flex-grow resize-none min-h-[300px] max-h-[calc(100vh-300px)] rounded-lg p-3 text-text-default "
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder={`Enter ${infoLabel.toLowerCase()}...`}
          />
        </div>
        <Button
          onClick={onSave}
          className="w-full h-[60px] rounded-none border-b border-border-subtle bg-transparent hover:bg-background-medium text-text-default font-medium text-base"
        >
          Save Changes
        </Button>
        <Button
          onClick={onClose}
          variant="ghost"
          className="w-full h-[60px] rounded-none hover:bg-background-medium text-text-muted hover:text-text-default text-base font-normal"
        >
          Cancel
        </Button>
      </div>
    </div>
  );
}
