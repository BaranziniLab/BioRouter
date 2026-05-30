import React, { useState, useEffect, useCallback } from 'react';
import { createPortal } from 'react-dom';
import { useForm } from '@tanstack/react-form';
import { Workflow, generateDeepLink, Parameter } from '../../workflow';
import { Check, ExternalLink, Play, Save, X } from '../icons/app-icons';
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
        settings: workflow.settings
          ? {
              biorouter_provider: workflow.settings.biorouter_provider ?? undefined,
              biorouter_model: workflow.settings.biorouter_model ?? undefined,
              temperature: workflow.settings.temperature ?? undefined,
            }
          : undefined,
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
      settings: undefined,
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
  const [settings, setSettings] = useState(form.state.values.settings);

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
      setSettings(form.state.values.settings);
    });
  }, [form]);
  const [copied, setCopied] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  // Extensions for the workflow (editable)
  const [workflowExtensions, setWorkflowExtensions] = useState<ExtensionConfig[]>(
    () => workflow?.extensions ?? []
  );

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

    // Build settings — only include if at least one value is set
    const settingsConfig =
      settings?.biorouter_provider || settings?.biorouter_model || settings?.temperature !== undefined
        ? {
            biorouter_provider: settings?.biorouter_provider || undefined,
            biorouter_model: settings?.biorouter_model || undefined,
            temperature: settings?.temperature,
          }
        : undefined;

    return {
      ...workflow,
      title,
      description,
      instructions,
      activities,
      prompt: prompt || undefined,
      parameters: formattedParameters,
      response: responseConfig,
      settings: settingsConfig,
      // Strip envs to avoid leaking secrets
      extensions: workflowExtensions.length > 0
        ? workflowExtensions.map((extension) =>
            'envs' in extension ? { ...extension, envs: undefined } : extension
          ) as ExtensionConfig[]
        : undefined,
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
    settings,
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
    settings,
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

  // ESC-to-close: ensures the modal stays dismissible even when its host
  // chat window is too small to show the in-modal Close button (matters
  // most in dashboard mode where chat windows can be shrunk down).
  useEffect(() => {
    if (!isOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  // Portal to document.body so the modal escapes any ancestor that
  // establishes a containing block for `fixed` (the dashboard world
  // layer uses translate3d + will-change, which would otherwise clip
  // the overlay to the small chat window).
  return createPortal(
    <div
      className="fixed inset-0 z-[400] flex items-center justify-center bg-black/50"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose(false);
      }}
    >
      <div className="bg-background-default border border-border-subtle rounded-lg w-[90vw] max-w-4xl h-[90vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-5 border-b border-border-subtle flex-shrink-0">
          <div>
            <h1 className="text-base font-semibold text-text-default">
              {isCreateMode ? 'Create Workflow' : 'Edit Workflow'}
            </h1>
            <p className="text-xs text-text-muted mt-0.5">
              {isCreateMode
                ? 'Define agent behavior and capabilities for reusable chat sessions.'
                : "Edit the workflow to change agent behavior."}{' '}
              <a
                href="https://baranzinilab.github.io/biorouter-landing/docs.html"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-0.5 text-[#cf6d47] hover:underline"
              >
                Learn more
                <ExternalLink className="w-3 h-3" />
              </a>
            </p>
          </div>
          <Button
            onClick={() => onClose(false)}
            variant="ghost"
            size="sm"
            className="p-1.5 hover:bg-background-medium rounded-lg transition-colors"
          >
            <X className="w-4 h-4" />
          </Button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-y-auto px-6 py-4">
          <WorkflowFormFields
            form={form}
            extensions={workflowExtensions}
            onExtensionsChange={setWorkflowExtensions}
          />

          {/* Deep Link Display */}
          {requiredFieldsAreFilled() && (
            <div className="w-full p-4 bg-background-medium rounded-lg mt-6">
              <div className="flex items-center justify-between mb-2">
                <div className="text-sm text-text-muted">
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
                  <span className="ml-1 text-sm text-text-muted">
                    {copied ? 'Copied!' : 'Copy'}
                  </span>
                </Button>
              </div>
              <div
                onClick={handleCopy}
                className="text-sm truncate font-mono cursor-pointer text-text-default"
              >
                {isGeneratingDeeplink
                  ? 'Generating deeplink...'
                  : deeplink || 'Click to generate deeplink'}
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between p-6 border-t border-border-subtle">
          <Button
            onClick={() => onClose(false)}
            variant="ghost"
            className="px-4 py-2 text-text-muted rounded-lg hover:bg-background-medium transition-colors"
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
    </div>,
    document.body
  );
}
