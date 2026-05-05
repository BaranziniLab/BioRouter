import { useState } from 'react';
import { useForm } from '@tanstack/react-form';
import { z } from 'zod';
import { Download } from 'lucide-react';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { Workflow, decodeWorkflow } from '../../workflow';
import { toastSuccess, toastError } from '../../toasts';
import { useEscapeKey } from '../../hooks/useEscapeKey';
import { getWorkflowJsonSchema } from '../../workflow/validation';
import { saveWorkflow } from '../../workflow/workflow_management';
import { parseWorkflow } from '../../api';

interface ImportWorkflowFormProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess: () => void;
}

// Define Zod schema for the import form
const importWorkflowSchema = z
  .object({
    deeplink: z
      .string()
      .refine(
        (value) => !value || value.trim().startsWith('biorouter://workflow?config='),
        'Invalid deeplink format. Expected: biorouter://workflow?config=...'
      ),
    workflowUploadFile: z
      .instanceof(File)
      .nullable()
      .refine((file) => {
        if (!file) return true;
        return file.size <= 1024 * 1024;
      }, 'File is too large, max size is 1MB'),
  })
  .refine((data) => (data.deeplink && data.deeplink.trim()) || data.workflowUploadFile, {
    message: 'Either of deeplink or workflow file are required',
    path: ['deeplink'],
  });

export default function ImportWorkflowForm({ isOpen, onClose, onSuccess }: ImportWorkflowFormProps) {
  const [importing, setImporting] = useState(false);
  const [showSchemaModal, setShowSchemaModal] = useState(false);

  useEscapeKey(isOpen, onClose);

  const parseDeeplink = async (deeplink: string): Promise<Workflow | null> => {
    try {
      const cleanLink = deeplink.trim();

      if (!cleanLink.startsWith('biorouter://workflow?config=')) {
        throw new Error('Invalid deeplink format. Expected: biorouter://workflow?config=...');
      }

      const workflowEncoded = cleanLink.replace('biorouter://workflow?config=', '');

      if (!workflowEncoded) {
        throw new Error('No workflow configuration found in deeplink');
      }
      const workflow = await decodeWorkflow(workflowEncoded);

      if (!workflow.title || !workflow.description) {
        throw new Error('Workflow is missing required fields (title, description)');
      }

      if (!workflow.instructions && !workflow.prompt) {
        throw new Error('Workflow must have either instructions or prompt');
      }

      return workflow;
    } catch (error) {
      console.error('Failed to parse deeplink:', error);
      return null;
    }
  };

  const parseWorkflowFromFile = async (fileContent: string): Promise<Workflow> => {
    try {
      let response = await parseWorkflow({
        body: {
          content: fileContent,
        },
        throwOnError: true,
      });
      return response.data.workflow;
    } catch (error) {
      let error_message = 'unknown error';
      if (typeof error === 'object' && error !== null && 'message' in error) {
        error_message = error.message as string;
      }
      throw new Error(error_message);
    }
  };

  const importWorkflowForm = useForm({
    defaultValues: {
      deeplink: '',
      workflowUploadFile: null as File | null,
    },
    validators: {
      onChange: importWorkflowSchema,
    },
    onSubmit: async ({ value }) => {
      setImporting(true);
      try {
        let workflow: Workflow;

        // Parse workflow from either deeplink or workflow file
        if (value.deeplink && value.deeplink.trim()) {
          const parsedWorkflow = await parseDeeplink(value.deeplink.trim());
          if (!parsedWorkflow) {
            throw new Error('Invalid deeplink or workflow format');
          }
          workflow = parsedWorkflow;
        } else {
          const fileContent = await value.workflowUploadFile!.text();
          workflow = await parseWorkflowFromFile(fileContent);
        }

        await saveWorkflow(workflow, null);

        // Reset dialog state
        importWorkflowForm.reset({
          deeplink: '',
          workflowUploadFile: null,
        });
        onClose();

        onSuccess();

        toastSuccess({
          title: workflow.title.trim(),
          msg: 'Workflow imported successfully',
        });
      } catch (error) {
        console.error('Failed to import workflow:', error);

        toastError({
          title: 'Import Failed',
          msg: `Failed to import workflow: ${error instanceof Error ? error.message : 'Unknown error'}`,
          traceback: error instanceof Error ? error.message : String(error),
        });
      } finally {
        setImporting(false);
      }
    },
  });

  const handleClose = () => {
    importWorkflowForm.reset({
      deeplink: '',
      workflowUploadFile: null,
    });
    onClose();
  };

  const handleDeeplinkChange = async (
    value: string,
    field: { handleChange: (value: string) => void }
  ) => {
    field.handleChange(value);

    if (value.trim()) {
      try {
        await parseDeeplink(value.trim());
      } catch (error) {
        toastError({
          title: 'Invalid Deeplink',
          msg: `The deeplink format is invalid: ${error instanceof Error ? error.message : 'Unknown error'}`,
        });
      }
    }
  };

  const handleWorkflowUploadChange = async (file: File | undefined) => {
    importWorkflowForm.setFieldValue('workflowUploadFile', file || null);

    if (file) {
      try {
        const fileContent = await file.text();
        await parseWorkflowFromFile(fileContent);
      } catch (error) {
        toastError({
          title: 'Invalid Workflow File',
          msg: error instanceof Error ? error.message : 'Unknown error',
        });
      }
    }
  };

  if (!isOpen) return null;

  return (
    <>
      <div className="fixed inset-0 z-[300] flex items-center justify-center bg-black/50">
        <div className="bg-background-default border border-border-subtle rounded-lg p-6 w-[500px] max-w-[90vw]">
          <h3 className="text-lg font-medium text-text-standard mb-4">Import Workflow</h3>

          <form
            onSubmit={(e) => {
              e.preventDefault();
              e.stopPropagation();
              importWorkflowForm.handleSubmit();
            }}
          >
            <div className="space-y-4">
              <importWorkflowForm.Subscribe selector={(state) => state.values}>
                {(values) => (
                  <>
                    <importWorkflowForm.Field name="deeplink">
                      {(field) => {
                        const isDisabled = values.workflowUploadFile !== null;

                        return (
                          <div className={isDisabled ? 'opacity-50' : ''}>
                            <label
                              htmlFor="import-deeplink"
                              className="block text-sm font-medium text-text-standard mb-2"
                            >
                              Workflow Deeplink
                            </label>
                            <textarea
                              id="import-deeplink"
                              value={field.state.value}
                              onChange={(e) => handleDeeplinkChange(e.target.value, field)}
                              onBlur={field.handleBlur}
                              disabled={isDisabled}
                              className={`w-full p-3 border rounded-lg bg-background-default text-text-standard focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none ${
                                field.state.meta.errors.length > 0
                                  ? 'border-red-500'
                                  : 'border-border-subtle'
                              } ${isDisabled ? 'cursor-not-allowed bg-gray-40 text-gray-300' : ''}`}
                              placeholder="Paste your biorouter://workflow?config=... deeplink here"
                              rows={3}
                              autoFocus={!isDisabled}
                            />
                            <p
                              className={`text-xs mt-1 ${isDisabled ? 'text-gray-300' : 'text-text-muted'}`}
                            >
                              Paste a workflow deeplink starting with "biorouter://workflow?config="
                            </p>
                            {field.state.meta.errors.length > 0 && (
                              <p className="text-red-500 text-sm mt-1">
                                {typeof field.state.meta.errors[0] === 'string'
                                  ? field.state.meta.errors[0]
                                  : field.state.meta.errors[0]?.message ||
                                    String(field.state.meta.errors[0])}
                              </p>
                            )}
                          </div>
                        );
                      }}
                    </importWorkflowForm.Field>

                    <div className="relative">
                      <div className="absolute inset-0 flex items-center">
                        <div className="w-full border-t border-border-subtle" />
                      </div>
                      <div className="relative flex justify-center text-sm">
                        <span className="px-3 bg-background-default text-text-muted font-medium">
                          OR
                        </span>
                      </div>
                    </div>

                    <importWorkflowForm.Field name="workflowUploadFile">
                      {(field) => {
                        const hasDeeplink = values.deeplink?.trim();
                        const isDisabled = !!hasDeeplink;

                        return (
                          <div className={isDisabled ? 'opacity-50' : ''}>
                            <label
                              htmlFor="import-workflow-file"
                              className="block text-sm font-medium text-text-standard mb-3"
                            >
                              Workflow File
                            </label>
                            <div className="relative">
                              <Input
                                id="import-workflow-file"
                                type="file"
                                accept=".yaml,.yml,.json"
                                disabled={isDisabled}
                                onChange={(e) => {
                                  handleWorkflowUploadChange(e.target.files?.[0]);
                                }}
                                onBlur={field.handleBlur}
                                className={`file:pt-1 ${field.state.meta.errors.length > 0 ? 'border-red-500' : ''} ${
                                  isDisabled ? 'cursor-not-allowed' : ''
                                }`}
                              />
                            </div>
                            <div className="flex items-center justify-between">
                              <p
                                className={`text-xs mt-1 ${isDisabled ? 'text-gray-300' : 'text-text-muted'}`}
                              >
                                Upload a YAML or JSON file containing the workflow structure
                              </p>
                              <button
                                type="button"
                                onClick={() => setShowSchemaModal(true)}
                                className="text-xs text-blue-500 hover:text-blue-700 underline"
                                disabled={isDisabled}
                              >
                                example
                              </button>
                            </div>
                            {field.state.meta.errors.length > 0 && (
                              <p className="text-red-500 text-sm mt-1">
                                {typeof field.state.meta.errors[0] === 'string'
                                  ? field.state.meta.errors[0]
                                  : field.state.meta.errors[0]?.message ||
                                    String(field.state.meta.errors[0])}
                              </p>
                            )}
                          </div>
                        );
                      }}
                    </importWorkflowForm.Field>
                  </>
                )}
              </importWorkflowForm.Subscribe>

              <p className="text-xs text-text-muted">
                Ensure you review contents of workflow files before adding them to your biorouter
                interface.
              </p>
            </div>

            <div className="flex justify-end space-x-3 mt-6">
              <Button type="button" onClick={handleClose} variant="ghost" disabled={importing}>
                Cancel
              </Button>
              <importWorkflowForm.Subscribe
                selector={(state) => [state.canSubmit, state.isSubmitting]}
              >
                {([canSubmit, isSubmitting]) => (
                  <Button
                    type="submit"
                    disabled={!canSubmit || importing || isSubmitting}
                    variant="default"
                  >
                    {importing || isSubmitting ? 'Importing...' : 'Import Workflow'}
                  </Button>
                )}
              </importWorkflowForm.Subscribe>
            </div>
          </form>
        </div>
      </div>

      {/* Schema Modal */}
      {showSchemaModal && (
        <div className="fixed inset-0 z-[400] flex items-center justify-center bg-black/50">
          <div className="bg-background-default border border-border-subtle rounded-lg p-6 w-[800px] max-w-[90vw] max-h-[80vh] flex flex-col">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-medium text-text-standard">Expected Workflow Structure</h3>
              <button
                type="button"
                onClick={() => setShowSchemaModal(false)}
                className="text-text-muted hover:text-text-standard"
              >
                ✕
              </button>
            </div>
            <p className="mt-4 text-blue-700 text-sm">
              Your YAML or JSON file should follow this structure. Required fields are: title,
              description, and either instructions or prompt.
            </p>
            <div className="flex-1 overflow-auto">
              <pre className="text-xs bg-whitedark:bg-gray-800 p-4 rounded overflow-auto whitespace-pre font-mono">
                {JSON.stringify(getWorkflowJsonSchema(), null, 2)}
              </pre>
            </div>
          </div>
        </div>
      )}
    </>
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
