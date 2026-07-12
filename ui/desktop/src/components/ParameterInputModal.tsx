import React, { useState, useEffect } from 'react';
import { Parameter } from '../workflow';
import { Button } from './ui/button';
import { getInitialWorkingDir } from '../utils/workingDir';
import { Dialog, DialogContent, DialogTitle } from './ui/dialog';

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

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (open) return;
        if (showCancelOptions) setShowCancelOptions(false);
        else handleCancel();
      }}
    >
      <DialogContent
        showCloseButton={false}
        className={`p-0 overflow-hidden ${
          showCancelOptions ? 'max-w-md sm:max-w-md' : 'max-h-[90vh] max-w-lg sm:max-w-lg'
        }`}
      >
        {showCancelOptions ? (
          <div className="p-8">
            <DialogTitle className="mb-4">Cancel Workflow Setup</DialogTitle>
            <p className="text-text-default mb-6">What would you like to do?</p>
            <div className="flex flex-col gap-3">
              <Button
                autoFocus
                onClick={() => handleCancelOption('back-to-form')}
                variant="default"
                size="lg"
                className="w-full rounded-md"
              >
                Back to Parameter Form
              </Button>
              <Button
                onClick={() => handleCancelOption('new-chat')}
                variant="outline"
                size="lg"
                className="w-full rounded-md"
              >
                Start New Chat (No Workflow)
              </Button>
            </div>
          </div>
        ) : (
          <div className="flex max-h-[90vh] flex-col overflow-hidden">
            <div className="p-8 pb-4 flex-shrink-0">
              <DialogTitle className="mb-6">Workflow Parameters</DialogTitle>
            </div>
            <div className="flex-1 overflow-y-auto px-8">
              <form onSubmit={handleSubmit} className="space-y-4 mb-4">
                {parameters.map((param) => (
                  <div key={param.key}>
                    <label className="block text-sm font-medium text-text-default mb-2">
                      {param.description || param.key}
                      {param.requirement === 'required' && (
                        <span className="text-text-danger ml-1">*</span>
                      )}
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
            </div>
            <div className="p-8 pt-4 flex-shrink-0">
              <div className="flex justify-end gap-4">
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
                  Start Workflow
                </Button>
              </div>
            </div>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
};

export default ParameterInputModal;
