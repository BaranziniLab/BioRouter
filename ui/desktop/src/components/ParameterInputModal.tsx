import React, { useState, useEffect } from 'react';
import { Parameter } from '../workflow';
import { Button } from './ui/button';
import { getInitialWorkingDir } from '../utils/workingDir';
import { ModalShell } from './ModalShell';

interface ParameterInputModalProps {
  parameters: Parameter[];
  onSubmit: (values: Record<string, string>) => void;
  onClose: () => void;
  initialValues?: Record<string, string>;
}

const ParameterInputModal: React.FC<ParameterInputModalProps> = ({
  parameters,
  onSubmit,
  onClose,
  initialValues,
}) => {
  const [inputValues, setInputValues] = useState<Record<string, string>>({});
  const [validationErrors, setValidationErrors] = useState<Record<string, string>>({});
  const [showCancelOptions, setShowCancelOptions] = useState(false);

  // Pre-fill the form with default values from the workflow and initialValues from deeplink
  useEffect(() => {
    const defaultValues: Record<string, string> = {};
    parameters.forEach((param) => {
      if (param.requirement === 'optional' && param.default) {
        defaultValues[param.key] =
          param.input_type === 'boolean' ? param.default.toLowerCase() : param.default;
      }
    });

    setInputValues({ ...defaultValues, ...initialValues });
  }, [parameters, initialValues]);

  const handleChange = (name: string, value: string): void => {
    setInputValues((prevValues: Record<string, string>) => ({ ...prevValues, [name]: value }));
  };

  const handleSubmit = (): void => {
    // Clear previous validation errors
    setValidationErrors({});

    // Check if all *required* parameters are filled
    const requiredParams: Parameter[] = parameters.filter((p) => p.requirement === 'required');
    const errors: Record<string, string> = {};

    requiredParams.forEach((param) => {
      const value = inputValues[param.key]?.trim();
      if (!value) {
        errors[param.key] = `${param.description || param.key} is required`;
      }
    });

    if (Object.keys(errors).length > 0) {
      setValidationErrors(errors);
      return;
    }

    onSubmit(inputValues);
  };

  const handleCancel = (): void => {
    // Always show cancel options if workflow has any parameters (required or optional)
    const hasAnyParams = parameters.length > 0;

    if (hasAnyParams) {
      setShowCancelOptions(true);
    } else {
      onClose();
    }
  };

  const handleCancelOption = (option: 'new-chat' | 'back-to-form'): void => {
    if (option === 'new-chat') {
      try {
        const workingDir = getInitialWorkingDir();
        window.electron.createChatWindow(undefined, workingDir);
        window.electron.hideWindow();
      } catch (error) {
        console.error('Error creating new window:', error);
        onClose();
      }
    } else {
      setShowCancelOptions(false); // Go back to the parameter form
    }
  };

  // Two states, one shell. The cancel prompt is a confirmation (width S,
  // dismissible — backing out of it is free); the parameter form is a form
  // (width M) whose typed values a stray backdrop click must not discard.
  if (showCancelOptions) {
    return (
      <ModalShell
        open
        onOpenChange={(open) => !open && setShowCancelOptions(false)}
        size="sm"
        purpose="info"
        title="Cancel workflow setup?"
        footer={
          <>
            <Button
              onClick={() => handleCancelOption('new-chat')}
              variant="outline"
              size="sm"
              className="rounded-md"
            >
              Start a new chat
            </Button>
            <Button
              autoFocus
              onClick={() => handleCancelOption('back-to-form')}
              variant="default"
              size="sm"
              className="rounded-md"
            >
              Back to the form
            </Button>
          </>
        }
      >
        <p className="py-3 text-body text-text-muted">
          Your parameter values have not been submitted yet.
        </p>
      </ModalShell>
    );
  }

  return (
    <ModalShell
      open
      onOpenChange={(open) => !open && handleCancel()}
      size="md"
      purpose="form"
      scrollBody
      title="Workflow parameters"
      footer={
        <>
          <Button
            type="button"
            onClick={handleCancel}
            variant="outline"
            size="default"
            className="rounded-md"
          >
            Cancel
          </Button>
          <Button
            type="button"
            onClick={handleSubmit}
            variant="default"
            size="default"
            className="rounded-md"
          >
            Start workflow
          </Button>
        </>
      }
    >
      <form onSubmit={handleSubmit} className="space-y-4 py-3">
        {parameters.map((param) => (
          <div key={param.key}>
            <label className="block text-sm font-medium text-text-default mb-2">
              {param.description || param.key}
              {param.requirement === 'required' && <span className="text-text-danger ml-1">*</span>}
            </label>

            {/* Render different input types */}
            {param.input_type === 'select' && param.options ? (
              <select
                value={inputValues[param.key] || ''}
                onChange={(e) => handleChange(param.key, e.target.value)}
                className={`w-full p-3 border rounded-lg bg-background-medium text-text-default ${validationErrors[param.key] ? 'border-border-danger ' : 'border-border-subtle '}`}
              >
                <option value="">Select an option...</option>
                {param.options.map((option) => (
                  <option key={option} value={option}>
                    {option}
                  </option>
                ))}
              </select>
            ) : param.input_type === 'boolean' ? (
              <select
                value={inputValues[param.key] || ''}
                onChange={(e) => handleChange(param.key, e.target.value)}
                className={`w-full p-3 border rounded-lg bg-background-medium text-text-default ${validationErrors[param.key] ? 'border-border-danger ' : 'border-border-subtle '}`}
              >
                <option value="">Select...</option>
                <option value="true">True</option>
                <option value="false">False</option>
              </select>
            ) : (
              <input
                type={param.input_type === 'number' ? 'number' : 'text'}
                value={inputValues[param.key] || ''}
                onChange={(e) => handleChange(param.key, e.target.value)}
                className={`w-full p-3 border rounded-lg bg-background-medium text-text-default ${validationErrors[param.key] ? 'border-border-danger ' : 'border-border-subtle '}`}
                placeholder={param.default || `Enter value for ${param.key}...`}
              />
            )}

            {validationErrors[param.key] && (
              <p className="text-text-danger text-sm mt-1">{validationErrors[param.key]}</p>
            )}
          </div>
        ))}
      </form>
    </ModalShell>
  );
};

export default ParameterInputModal;
