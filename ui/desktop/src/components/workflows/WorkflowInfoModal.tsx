import React, { useEffect, useRef, useState } from 'react';
import { Card } from '../ui/card';
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
    <div className="fixed inset-0 bg-black/20 dark:bg-white/20 backdrop-blur-sm transition-colors animate-[fadein_200ms_ease-in_forwards] z-[1000]">
      <Card className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 flex flex-col min-w-[80%] min-h-[80%] bg-background-default rounded-xl overflow-hidden shadow-lg px-8 pt-[24px] pb-0">
        <div className="flex mb-6">
          <h2 className="text-base font-semibold text-text-default">Edit {infoLabel}</h2>
        </div>
        <div className="flex flex-col flex-grow overflow-y-auto space-y-8">
          <textarea
            ref={textareaRef}
            className="w-full flex-grow resize-none min-h-[300px] max-h-[calc(100vh-300px)] border border-border-subtle rounded-lg p-3 text-text-default bg-background-default focus:outline-none focus:ring-1 focus:ring-borderProminent focus:border-border-strong"
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
      </Card>
    </div>
  );
}
