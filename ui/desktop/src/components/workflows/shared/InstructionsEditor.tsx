import React, { useState } from 'react';
import { X } from '../../icons/app-icons';
import { Button } from '../../ui/button';
import { useEscapeKey } from '../../../hooks/useEscapeKey';

interface InstructionsEditorProps {
  isOpen: boolean;
  onClose: () => void;
  value: string;
  onChange: (value: string) => void;
  error?: string;
}

export default function InstructionsEditor({
  isOpen,
  onClose,
  value,
  onChange,
  error,
}: InstructionsEditorProps) {
  const [localValue, setLocalValue] = useState(value);
  useEscapeKey(isOpen, onClose);

  React.useEffect(() => {
    if (isOpen) {
      setLocalValue(value);
    }
  }, [isOpen, value]);

  const handleSave = () => {
    onChange(localValue);
    onClose();
  };

  const handleCancel = () => {
    setLocalValue(value); // Reset to original value
    onClose();
  };

  const insertExample = () => {
    const example = `You are an AI assistant helping with {{task_type}}. 

Please follow these steps:
1. Analyze the provided {{input_data}}
2. Apply the specified {{methodology}} 
3. Generate a comprehensive report

Requirements:
- Be thorough and accurate
- Use clear, professional language
- Include specific examples where relevant
- Provide actionable recommendations

Format your response with:
- Executive summary
- Detailed analysis
- Key findings
- Next steps

Use {{parameter_name}} syntax for any user-provided values.`;
    setLocalValue(example);
  };

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-[400] flex items-center justify-center bg-black/50"
      onClick={(e) => {
        // Close modal when clicking backdrop
        if (e.target === e.currentTarget) {
          handleCancel();
        }
      }}
    >
      <div className="bg-background-default border border-border-subtle rounded-lg p-6 w-[900px] max-w-[90vw] max-h-[90vh] overflow-hidden flex flex-col">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-base font-semibold text-text-default">Instructions Editor</h3>
          <Button type="button" variant="ghost" size="sm" onClick={handleCancel}>
            <X className="w-4 h-4" />
          </Button>
        </div>

        <div className="flex-1 flex flex-col min-h-0">
          <div className="mb-4">
            <div className="flex items-center justify-between mb-2">
              <label className="block text-sm font-medium text-text-default">Instructions</label>
              <Button
                type="button"
                onClick={insertExample}
                variant="ghost"
                size="sm"
                className="text-xs"
              >
                Insert Example
              </Button>
            </div>
            <p className="text-xs text-text-muted mb-3">
              Use <code className="bg-background-muted px-1 rounded">{`{{parameter_name}}`}</code>{' '}
              syntax to define parameters that users can fill in
            </p>
          </div>

          <div className="flex-1 min-h-0">
            <textarea
              value={localValue}
              onChange={(e) => setLocalValue(e.target.value)}
              className={`w-full h-full min-h-[500px] px-3 py-2 text-sm border rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150 resize-none font-mono ${
                error ? 'border-red-500 dark:border-red-400' : 'border-border-subtle'
              }`}
              placeholder="Detailed instructions for the AI, hidden from the user"
            />
            {error && <p className="text-red-500 dark:text-red-400 text-sm mt-2">{error}</p>}
          </div>
        </div>

        <div className="flex justify-end space-x-3 mt-6 pt-4 border-t border-border-subtle">
          <Button type="button" onClick={handleCancel} variant="ghost">
            Cancel
          </Button>
          <Button type="button" onClick={handleSave} variant="default">
            Save Instructions
          </Button>
        </div>
      </div>
    </div>
  );
}
