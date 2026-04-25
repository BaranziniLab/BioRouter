import React, { useState, useEffect, useCallback } from 'react';
import { useForm } from '@tanstack/react-form';
import { Workflow, generateDeepLink, Parameter } from '../../workflow';
import { Check, ExternalLink, Play, Save, X } from 'lucide-react';
import { BioRouterIcon } from '../icons/BioRouterIcon';
import Copy from '../icons/Copy';
import { ExtensionConfig } from '../ConfigContext';
import { Button } from '../ui/button';

import { WorkflowFormFields } from './shared/WorkflowFormFields';
import { WorkflowFormData } from './shared/workflowFormSchema';
import { toastSuccess, toastError } from '../../toasts';
import { saveWorkflow } from '../../workflow/workflow_management';

interface CreateEditWorkflowModalProps {
  isOpen: boolean;
  onClose: (wasSaved?: boolean) => void;
  workflow?: Workflow;
  isCreateMode?: boolean;
  workflowId?: string | null;
}

export default function CreateEditWorkflowModal({
  isOpen,
  onClose,
  workflow,
  isCreateMode = false,
  workflowId,
}: CreateEditWorkflowModalProps) {
  const getInitialValues = React.useCallback((): WorkflowFormData => {
    if (workflow) {
      return {
        title: workflow.title || '',
        description: workflow.description || '',
        instructions: workflow.instructions || '',
        prompt: workflow.prompt || '',
        activities: workflow.activities || [],
        parameters: workflow.parameters || [],
        jsonSchema: workflow.response?.json_schema
          ? JSON.stringify(workflow.response.json_schema, null, 2)
          : '',
      };
    }
    return {
      title: '',
      description: '',
      instructions: '',
      prompt: '',
      activities: [],
      parameters: [],
      jsonSchema: '',
    };
  }, [workflow]);

  const form = useForm({
    defaultValues: getInitialValues(),
  });

  // Helper functions to get values from form - using state to trigger re-renders
  const [title, setTitle] = useState(form.state.values.title);
  const [description, setDescription] = useState(form.state.values.description);
  const [instructions, setInstructions] = useState(form.state.values.instructions);
  const [prompt, setPrompt] = useState(form.state.values.prompt);
  const [activities, setActivities] = useState(form.state.values.activities);
  const [parameters, setParameters] = useState(form.state.values.parameters);
  const [jsonSchema, setJsonSchema] = useState(form.state.values.jsonSchema);

  // Subscribe to form changes to update local state
  useEffect(() => {
    return form.store.subscribe(() => {
      setTitle(form.state.values.title);
      setDescription(form.state.values.description);
      setInstructions(form.state.values.instructions);
      setPrompt(form.state.values.prompt);
      setActivities(form.state.values.activities);
      setParameters(form.state.values.parameters);
      setJsonSchema(form.state.values.jsonSchema);
    });
  }, [form]);
  const [copied, setCopied] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  // Initialize selected extensions for the workflow
  const [workflowExtensions] = useState<ExtensionConfig[]>(() => {
    if (workflow?.extensions) {
      return workflow.extensions;
    }
    return [];
  });

  // Reset form when workflow changes
  useEffect(() => {
    if (workflow) {
      const newValues = getInitialValues();
      form.reset(newValues);
    }
  }, [workflow, form, getInitialValues]);

  const getCurrentWorkflow = useCallback((): Workflow => {
    // Transform the internal parameters state into the desired output format.
    const formattedParameters = parameters.map((param) => {
      const formattedParam: Parameter = {
        key: param.key,
        input_type: param.input_type || 'string',
        requirement: param.requirement,
        description: param.description,
      };

      // Add the 'default' key ONLY if the parameter is optional and has a default value.
      if (param.requirement === 'optional' && param.default) {
        formattedParam.default = param.default;
      }

      // Add options for select input type
      if (param.input_type === 'select' && param.options) {
        formattedParam.options = param.options.filter((opt) => opt.trim() !== ''); // Filter empty options when saving
      }

      return formattedParam;
    });

    // Parse response schema if provided
    let responseConfig = undefined;
    if (jsonSchema && jsonSchema.trim()) {
      try {
        const parsedSchema = JSON.parse(jsonSchema);
        responseConfig = { json_schema: parsedSchema };
      } catch (error) {
        console.warn('Invalid JSON schema provided:', error);
        // If JSON is invalid, don't include response config
      }
    }

    return {
      ...workflow,
      title,
      description,
      instructions,
      activities,
      prompt: prompt || undefined,
      parameters: formattedParameters,
      response: responseConfig,
      // Strip envs to avoid leaking secrets
      extensions: workflowExtensions.map((extension) =>
        'envs' in extension ? { ...extension, envs: undefined } : extension
      ) as ExtensionConfig[],
    };
  }, [
    workflow,
    title,
    description,
    instructions,
    activities,
    prompt,
    parameters,
    jsonSchema,
    workflowExtensions,
  ]);

  const requiredFieldsAreFilled = () => {
    return title.trim() && description.trim() && (instructions.trim() || (prompt || '').trim());
  };

  const validateForm = () => {
    const basicValidation =
      title.trim() && description.trim() && (instructions.trim() || (prompt || '').trim());

    // If JSON schema is provided, it must be valid
    if (jsonSchema && jsonSchema.trim()) {
      try {
        JSON.parse(jsonSchema);
      } catch {
        return false; // Invalid JSON schema fails validation
      }
    }

    return basicValidation;
  };

  const [deeplink, setDeeplink] = useState('');
  const [isGeneratingDeeplink, setIsGeneratingDeeplink] = useState(false);

  // Generate deeplink whenever workflow configuration changes
  useEffect(() => {
    let isCancelled = false;

    const generateLink = async () => {
      if (
        !title.trim() ||
        !description.trim() ||
        (!instructions.trim() && !(prompt || '').trim())
      ) {
        setDeeplink('');
        return;
      }

      setIsGeneratingDeeplink(true);
      try {
        const currentWorkflow = getCurrentWorkflow();
        const link = await generateDeepLink(currentWorkflow);
        if (!isCancelled) {
          setDeeplink(link);
        }
      } catch (error) {
        console.error('Failed to generate deeplink:', error);
        if (!isCancelled) {
          setDeeplink('Error generating deeplink');
        }
      } finally {
        if (!isCancelled) {
          setIsGeneratingDeeplink(false);
        }
      }
    };

    generateLink();

    return () => {
      isCancelled = true;
    };
  }, [
    title,
    description,
    instructions,
    prompt,
    activities,
    parameters,
    jsonSchema,
    workflowExtensions,
    getCurrentWorkflow,
  ]);

  const handleCopy = () => {
    if (!deeplink || isGeneratingDeeplink || deeplink === 'Error generating deeplink') {
      return;
    }

    navigator.clipboard
      .writeText(deeplink)
      .then(() => {
        setCopied(true);
        setTimeout(() => setCopied(false), 2000);
      })
      .catch((err) => {
        console.error('Failed to copy the text:', err);
      });
  };

  const handleSaveWorkflowClick = async () => {
    if (!validateForm()) {
      toastError({
        title: 'Validation Failed',
        msg: 'Please fill in all required fields and ensure JSON schema is valid.',
      });
      return;
    }

    setIsSaving(true);
    try {
      const workflow = getCurrentWorkflow();

      await saveWorkflow(workflow, workflowId);

      onClose(true);

      toastSuccess({
        title: (workflow.title || '').trim(),
        msg: 'Workflow saved successfully',
      });
    } catch (error) {
      console.error('Failed to save workflow:', error);

      toastError({
        title: 'Save Failed',
        msg: `Failed to save workflow: ${error instanceof Error ? error.message : 'Unknown error'}`,
        traceback: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsSaving(false);
    }
  };

  const handleSaveAndRunWorkflowClick = async () => {
    if (!validateForm()) {
      toastError({
        title: 'Validation Failed',
        msg: 'Please fill in all required fields and ensure JSON schema is valid.',
      });
      return;
    }

    setIsSaving(true);
    try {
      const workflow = getCurrentWorkflow();

      let saved_workflow_id = await saveWorkflow(workflow, workflowId);

      // Close modal first
      onClose(true);

      // Open workflow in a new window instead of navigating in the same window
      window.electron.createChatWindow(
        undefined,
        undefined,
        undefined,
        undefined,
        undefined,
        saved_workflow_id
      );

      toastSuccess({
        title: workflow.title,
        msg: 'Workflow saved and launched successfully',
      });
    } catch (error) {
      console.error('Failed to save and run workflow:', error);

      toastError({
        title: 'Save and Run Failed',
        msg: `Failed to save and run workflow: ${error instanceof Error ? error.message : 'Unknown error'}`,
        traceback: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setIsSaving(false);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-[400] flex items-center justify-center bg-black/50">
      <div className="bg-background-default border border-borderSubtle rounded-lg w-[90vw] max-w-4xl h-[90vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-6 border-b border-borderSubtle">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 bg-background-default rounded-full flex items-center justify-center">
              <BioRouterIcon className="w-6 h-6 text-iconProminent" />
            </div>
            <div>
              <h1 className="text-xl font-medium text-textProminent">
                {isCreateMode ? 'Create Workflow' : 'View/edit workflow'}
              </h1>
              <p className="text-textSubtle text-sm">
                {isCreateMode
                  ? 'Create a new workflow to define agent behavior and capabilities for reusable chat sessions.'
                  : "You can edit the workflow below to change the agent's behavior in a new session."}{' '}
                <a
                  href="https://github.com/BaranziniLab/BioRouter/docs/guides/workflows/"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex items-center gap-1 text-blue-500 hover:text-blue-600 hover:underline"
                >
                  Learn more
                  <ExternalLink className="w-3 h-3" />
                </a>
              </p>
            </div>
          </div>
          <Button
            onClick={() => onClose(false)}
            variant="ghost"
            size="sm"
            className="p-2 hover:bg-bgSubtle rounded-lg transition-colors"
          >
            <X className="w-5 h-5" />
          </Button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto px-6 py-4">
          <WorkflowFormFields form={form} />

          {/* Deep Link Display */}
          {requiredFieldsAreFilled() && (
            <div className="w-full p-4 bg-bgSubtle rounded-lg mt-6">
              <div className="flex items-center justify-between mb-2">
                <div className="text-sm text-textSubtle">
                  Copy this link to share with friends or paste directly in Chrome to open
                </div>
                <Button
                  onClick={handleCopy}
                  variant="ghost"
                  size="sm"
                  disabled={
                    !deeplink || isGeneratingDeeplink || deeplink === 'Error generating deeplink'
                  }
                  className="ml-4 p-2 hover:bg-background-default rounded-lg transition-colors flex items-center disabled:opacity-50 disabled:hover:bg-transparent"
                >
                  {copied ? (
                    <Check className="w-4 h-4 text-green-500" />
                  ) : (
                    <Copy className="w-4 h-4 text-iconSubtle" />
                  )}
                  <span className="ml-1 text-sm text-textSubtle">
                    {copied ? 'Copied!' : 'Copy'}
                  </span>
                </Button>
              </div>
              <div
                onClick={handleCopy}
                className="text-sm truncate font-mono cursor-pointer text-textStandard"
              >
                {isGeneratingDeeplink
                  ? 'Generating deeplink...'
                  : deeplink || 'Click to generate deeplink'}
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between p-6 border-t border-borderSubtle">
          <Button
            onClick={() => onClose(false)}
            variant="ghost"
            className="px-4 py-2 text-textSubtle rounded-lg hover:bg-bgSubtle transition-colors"
          >
            Close
          </Button>

          <div className="flex gap-3">
            <Button
              onClick={handleSaveWorkflowClick}
              disabled={!requiredFieldsAreFilled() || isSaving}
              variant="outline"
              size="default"
              className="inline-flex items-center justify-center gap-2 px-4 py-2"
            >
              <Save className="w-4 h-4" />
              {isSaving ? 'Saving...' : 'Save Workflow'}
            </Button>
            <Button
              onClick={handleSaveAndRunWorkflowClick}
              disabled={!requiredFieldsAreFilled() || isSaving}
              variant="default"
              size="default"
              className="inline-flex items-center justify-center gap-2 px-4 py-2"
            >
              <Play className="w-4 h-4" />
              {isSaving ? 'Saving...' : 'Save & Run Workflow'}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
