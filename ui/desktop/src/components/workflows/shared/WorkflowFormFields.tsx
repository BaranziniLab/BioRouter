import React, { useState, useEffect } from 'react';
import { Parameter } from '../../../workflow';
import { ChevronDown } from '../../icons/app-icons';

import ParameterInput from '../../parameter/ParameterInput';
import WorkflowActivityEditor from '../WorkflowActivityEditor';
import JsonSchemaEditor from './JsonSchemaEditor';
import InstructionsEditor from './InstructionsEditor';
import { Button } from '../../ui/button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '../../ui/collapsible';
import { WorkflowFormApi, WorkflowFormData } from './workflowFormSchema';
import { getExtensions } from '../../../api';
import type { ExtensionConfig } from '../../../api';

// Type for field API to avoid linting issues - use any to bypass complex type constraints
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type FormFieldApi<_T = any> = any;

interface WorkflowFormFieldsProps {
  // Form instance from parent
  form: WorkflowFormApi;

  // Extensions state managed by parent
  extensions?: ExtensionConfig[];
  onExtensionsChange?: (exts: ExtensionConfig[]) => void;

  // Event handlers
  onTitleChange?: (value: string) => void;
  onDescriptionChange?: (value: string) => void;
  onInstructionsChange?: (value: string) => void;
  onPromptChange?: (value: string) => void;
  onJsonSchemaChange?: (value: string) => void;
}

export const extractTemplateVariables = (content: string): string[] => {
  const templateVarRegex = /\{\{(.*?)\}\}/g;
  const variables: string[] = [];
  let match;

  while ((match = templateVarRegex.exec(content)) !== null) {
    const variable = match[1].trim();

    if (variable && !variables.includes(variable)) {
      // Filter out complex variables that aren't valid parameter names
      // This matches the backend logic in filter_complex_variables()
      const validVarRegex = /^\s*[a-zA-Z_][a-zA-Z0-9_]*\s*$/;
      if (validVarRegex.test(variable)) {
        variables.push(variable);
      }
    }
  }

  return variables;
};

export function WorkflowFormFields({
  form,
  extensions = [],
  onExtensionsChange,
  onTitleChange,
  onDescriptionChange,
  onInstructionsChange,
  onPromptChange,
  onJsonSchemaChange,
}: WorkflowFormFieldsProps) {
  const [showJsonSchemaEditor, setShowJsonSchemaEditor] = useState(false);
  const [showInstructionsEditor, setShowInstructionsEditor] = useState(false);
  const [newParameterName, setNewParameterName] = useState('');
  const [expandedParameters, setExpandedParameters] = useState<Set<string>>(new Set());
  const [availableExtensions, setAvailableExtensions] = useState<ExtensionConfig[]>([]);

  useEffect(() => {
    getExtensions({ throwOnError: false }).then((res) => {
      if (res.data?.extensions) {
        setAvailableExtensions(res.data.extensions.map((e) => ({ ...e })));
      }
    });
  }, []);

  // Force re-render when instructions, prompt, or activities change
  const [_forceRender, setForceRender] = useState(0);

  React.useEffect(() => {
    return form.store.subscribe(() => {
      // Force re-render when any form field changes to update parameter usage indicators
      setForceRender((prev) => prev + 1);
    });
  }, [form.store]);

  const parseParametersFromInstructions = React.useCallback(
    (instructions: string, prompt?: string, activities?: string[]): Parameter[] => {
      const instructionVars = extractTemplateVariables(instructions);
      const promptVars = prompt ? extractTemplateVariables(prompt) : [];
      const activityVars = activities
        ? activities.flatMap((activity) => extractTemplateVariables(activity))
        : [];

      // Combine and deduplicate
      const allVars = [...new Set([...instructionVars, ...promptVars, ...activityVars])];

      return allVars.map((key: string) => ({
        key,
        description: `Enter value for ${key}`,
        requirement: 'required' as const,
        input_type: 'string' as const,
      }));
    },
    []
  );

  // Function to update parameters based on current field values
  const updateParametersFromFields = React.useCallback(() => {
    const currentValues = form.state.values;
    const { instructions, prompt, activities, parameters: currentParams } = currentValues;

    const newParams = parseParametersFromInstructions(instructions, prompt, activities);

    // Separate manually added parameters (those not found in instructions/prompt/activities)
    const manualParams = currentParams.filter((param: Parameter) => {
      // Only keep manual params that have a valid key and are not found in the parsed params
      return (
        param.key && param.key.trim() && !newParams.some((newParam) => newParam.key === param.key)
      );
    });

    // Combine parsed parameters with manually added ones, filtering out empty ones
    const combinedParams = [
      ...newParams.map((newParam) => {
        const existing = currentParams.find((cp: Parameter) => cp.key === newParam.key);
        return existing ? { ...existing } : newParam;
      }),
      ...manualParams,
    ].filter((param: Parameter) => param.key && param.key.trim()) as Parameter[];

    // Only update if parameters actually changed
    const currentParamKeys = currentParams.map((p: Parameter) => p.key).sort();
    const newParamKeys = combinedParams.map((p) => p.key).sort();

    if (JSON.stringify(currentParamKeys) !== JSON.stringify(newParamKeys)) {
      form.setFieldValue('parameters', combinedParams);
    }
  }, [form, parseParametersFromInstructions]);

  const isParameterUsed = (
    paramKey: string,
    instructions: string,
    prompt?: string,
    activities?: string[]
  ): boolean => {
    const regex = new RegExp(
      `\\{\\{\\s*${paramKey.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\}\\}`,
      'g'
    );
    const usedInInstructions = regex.test(instructions);
    const usedInPrompt = prompt ? regex.test(prompt) : false;
    const usedInActivities = activities
      ? activities.some((activity) => {
          // For activities, we need to check the full activity string, including message: prefixes
          return regex.test(activity);
        })
      : false;
    return usedInInstructions || usedInPrompt || usedInActivities;
  };

  const checkHasAdvancedData = React.useCallback(
    (values: WorkflowFormData) => {
      const hasActivities = Boolean(values.activities && values.activities.length > 0);
      const hasParameters = Boolean(values.parameters && values.parameters.length > 0);
      const hasJsonSchema = Boolean(values.jsonSchema && values.jsonSchema.trim());
      const hasSettings = Boolean(
        values.settings?.biorouter_provider ||
          values.settings?.biorouter_model ||
          values.settings?.temperature !== undefined
      );
      const hasExtensions = extensions.length > 0;
      return hasActivities || hasParameters || hasJsonSchema || hasSettings || hasExtensions;
    },
    [extensions]
  );

  const [advancedOpen, setAdvancedOpen] = useState(() => checkHasAdvancedData(form.state.values));
  const hasAutoOpenedForExtensions = React.useRef(false);

  // Auto-open Advanced Options when pre-loaded extensions arrive (e.g. from session)
  useEffect(() => {
    if (extensions.length > 0 && !hasAutoOpenedForExtensions.current) {
      hasAutoOpenedForExtensions.current = true;
      setAdvancedOpen(true);
    }
  }, [extensions]);

  return (
    <div className="space-y-4" data-testid="workflow-form">
      {/* Title Field */}
      <form.Field name="title">
        {(field: FormFieldApi<string>) => (
          <div>
            <label
              htmlFor="workflow-title"
              className="block text-sm font-medium text-text-default mb-2"
            >
              Title <span className="text-red-500 dark:text-red-400">*</span>
            </label>
            <input
              id="workflow-title"
              type="text"
              value={field.state.value}
              onChange={(e) => {
                field.handleChange(e.target.value);
                onTitleChange?.(e.target.value);
              }}
              onBlur={field.handleBlur}
              className={`w-full h-9 px-3 text-sm border rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150 ${
                field.state.meta.errors.length > 0 ? 'border-red-500 dark:border-red-400' : 'border-border-subtle'
              }`}
              placeholder="Workflow title"
              data-testid="title-input"
            />
            {field.state.meta.errors.length > 0 && (
              <p className="text-red-500 dark:text-red-400 text-sm mt-1">{field.state.meta.errors[0]}</p>
            )}
          </div>
        )}
      </form.Field>

      {/* Description Field */}
      <form.Field name="description">
        {(field: FormFieldApi<string>) => (
          <div>
            <label
              htmlFor="workflow-description"
              className="block text-sm font-medium text-text-default mb-2"
            >
              Description <span className="text-red-500 dark:text-red-400">*</span>
            </label>
            <input
              id="workflow-description"
              type="text"
              value={field.state.value}
              onChange={(e) => {
                field.handleChange(e.target.value);
                onDescriptionChange?.(e.target.value);
              }}
              onBlur={field.handleBlur}
              className={`w-full h-9 px-3 text-sm border rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150 ${
                field.state.meta.errors.length > 0 ? 'border-red-500 dark:border-red-400' : 'border-border-subtle'
              }`}
              placeholder="Brief description of what this workflow does"
              data-testid="description-input"
            />
            {field.state.meta.errors.length > 0 && (
              <p className="text-red-500 dark:text-red-400 text-sm mt-1">{field.state.meta.errors[0]}</p>
            )}
          </div>
        )}
      </form.Field>

      {/* Instructions Field */}
      <form.Field name="instructions">
        {(field: FormFieldApi<string>) => (
          <div>
            <div className="flex items-center justify-between mb-2">
              <label
                htmlFor="workflow-instructions"
                className="block text-sm font-medium text-text-default"
              >
                Instructions <span className="text-red-500 dark:text-red-400">*</span>
              </label>
              <Button
                type="button"
                onClick={() => setShowInstructionsEditor(true)}
                variant="outline"
                size="sm"
                className="text-xs"
              >
                Open Editor
              </Button>
            </div>
            <textarea
              id="workflow-instructions"
              value={field.state.value}
              onChange={(e) => {
                field.handleChange(e.target.value);
                onInstructionsChange?.(e.target.value);
              }}
              onBlur={() => {
                field.handleBlur();
                updateParametersFromFields();
              }}
              className={`w-full px-3 py-2 text-sm border rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150 resize-none font-mono ${
                field.state.meta.errors.length > 0 ? 'border-red-500 dark:border-red-400' : 'border-border-subtle'
              }`}
              placeholder="Detailed instructions for the AI, hidden from the user"
              rows={8}
              data-testid="instructions-input"
            />
            <p className="text-xs text-text-muted mt-1">
              Use {`{{parameter_name}}`} to define parameters that can be filled in when running the
              workflow.
            </p>
            {field.state.meta.errors.length > 0 && (
              <p className="text-red-500 dark:text-red-400 text-sm mt-1">{field.state.meta.errors[0]}</p>
            )}

            {/* Instructions Editor Modal */}
            <InstructionsEditor
              isOpen={showInstructionsEditor}
              onClose={() => setShowInstructionsEditor(false)}
              value={field.state.value}
              onChange={(value) => {
                field.handleChange(value);
                onInstructionsChange?.(value);
                updateParametersFromFields();
              }}
              error={field.state.meta.errors.length > 0 ? field.state.meta.errors[0] : undefined}
            />
          </div>
        )}
      </form.Field>

      {/* Initial Prompt Field */}
      <form.Field name="prompt">
        {(field: FormFieldApi<string | undefined>) => (
          <div>
            <label
              htmlFor="workflow-prompt"
              className="block text-sm font-medium text-text-default mb-2"
            >
              Initial Prompt
            </label>
            <p className="text-xs text-text-muted mt-2 mb-2">
              (Optional - Instructions or Prompt are required)
            </p>
            <textarea
              id="workflow-prompt"
              value={field.state.value || ''}
              onChange={(e) => {
                field.handleChange(e.target.value);
                onPromptChange?.(e.target.value);
              }}
              onBlur={() => {
                field.handleBlur();
                updateParametersFromFields();
              }}
              className="w-full px-3 py-2 text-sm border border-border-subtle rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150 resize-none"
              placeholder="Pre-filled prompt when the workflow starts"
              rows={3}
              data-testid="prompt-input"
            />
          </div>
        )}
      </form.Field>

      {/* Advanced Section - Collapsible */}
      <Collapsible open={advancedOpen} onOpenChange={setAdvancedOpen} className="mt-6">
        <CollapsibleTrigger className="flex items-baseline gap-2 w-full py-3 px-4 bg-background-medium hover:bg-background-medium/80 rounded-lg transition-colors border border-border-subtle">
          <ChevronDown
            className={`w-4 h-4 text-text-muted transition-transform duration-200 flex-shrink-0 relative top-0.5 ${
              advancedOpen ? 'rotate-0' : '-rotate-90'
            }`}
          />
          <span className="text-sm font-medium text-text-default">Advanced Options</span>
          <span className="text-xs text-text-muted">Activities, parameters, model, extensions</span>
        </CollapsibleTrigger>

        <CollapsibleContent className="mt-4 space-y-4 pl-6 border-l-2 border-border-subtle ml-2">
          {/* Activities Field */}
          <form.Field name="activities">
            {(field: FormFieldApi<string[]>) => (
              <div>
                <WorkflowActivityEditor
                  activities={field.state.value}
                  setActivities={(activities) => field.handleChange(activities)}
                  onBlur={updateParametersFromFields}
                />
              </div>
            )}
          </form.Field>

          {/* Parameters Field */}
          <form.Field name="parameters">
            {(field: FormFieldApi<Parameter[]>) => {
              const handleAddParameter = () => {
                if (newParameterName.trim()) {
                  const newParam: Parameter = {
                    key: newParameterName.trim(),
                    description: `Enter value for ${newParameterName.trim()}`,
                    input_type: 'string',
                    requirement: 'required',
                  };
                  field.handleChange([...field.state.value, newParam]);
                  setNewParameterName('');
                  // Expand the newly added parameter by default
                  setExpandedParameters((prev) => {
                    const newSet = new Set(prev);
                    newSet.add(newParam.key);
                    return newSet;
                  });
                }
              };

              const handleKeyPress = (e: React.KeyboardEvent) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  handleAddParameter();
                }
              };

              const handleDeleteParameter = (parameterKey: string) => {
                const updatedParams = field.state.value.filter(
                  (param: Parameter) => param.key !== parameterKey
                );
                field.handleChange(updatedParams);
                // Remove from expanded set if it was expanded
                setExpandedParameters((prev) => {
                  const newSet = new Set(prev);
                  newSet.delete(parameterKey);
                  return newSet;
                });
              };

              const handleToggleExpanded = (parameterKey: string) => {
                setExpandedParameters((prev) => {
                  const newSet = new Set(prev);
                  if (newSet.has(parameterKey)) {
                    newSet.delete(parameterKey);
                  } else {
                    newSet.add(parameterKey);
                  }
                  return newSet;
                });
              };

              return (
                <div>
                  <label className="block text-sm font-medium text-text-default mb-1">
                    Parameters
                  </label>
                  <p className="text-text-muted text-sm space-y-2 pb-4">
                    Parameters will be automatically detected from {`{{parameter_name}}`} syntax in
                    instructions/prompt/activities or you can manually add them below.
                  </p>

                  {/* Add Parameter Input - Always Visible */}
                  <div className="flex gap-2 mb-4">
                    <input
                      type="text"
                      value={newParameterName}
                      onChange={(e) => setNewParameterName(e.target.value)}
                      onKeyPress={handleKeyPress}
                      placeholder="Parameter name…"
                      className="flex-1 h-9 px-3 text-sm border border-border-subtle rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150"
                    />
                    <Button
                      type="button"
                      onClick={handleAddParameter}
                      disabled={!newParameterName.trim()}
                      variant="outline"
                      size="sm"
                    >
                      Add
                    </Button>
                  </div>

                  {field.state.value.length > 0 &&
                    field.state.value
                      .filter((parameter: Parameter) => parameter.key && parameter.key.trim()) // Filter out empty parameters
                      .map((parameter: Parameter) => {
                        const currentValues = form.state.values;
                        const isUnused = !isParameterUsed(
                          parameter.key,
                          currentValues.instructions,
                          currentValues.prompt,
                          currentValues.activities
                        );

                        return (
                          <ParameterInput
                            key={parameter.key}
                            parameter={parameter}
                            isUnused={isUnused}
                            isExpanded={expandedParameters.has(parameter.key)}
                            onToggleExpanded={handleToggleExpanded}
                            onDelete={handleDeleteParameter}
                            onChange={(name, value) => {
                              const updatedParams = field.state.value.map((param: Parameter) =>
                                param.key === name ? { ...param, ...value } : param
                              );
                              field.handleChange(updatedParams);
                            }}
                          />
                        );
                      })}
                </div>
              );
            }}
          </form.Field>

          {/* JSON Schema Field */}
          <form.Field name="jsonSchema">
            {(field: FormFieldApi<string | undefined>) => (
              <div>
                <label className="block text-sm font-medium text-text-default mb-1">
                  Response JSON Schema
                </label>
                <p className="text-text-muted text-sm space-y-2 pb-4">
                  Define the expected structure of the AI's response using JSON Schema format
                </p>
                <div className="flex items-center justify-between mb-2">
                  <Button
                    type="button"
                    onClick={() => setShowJsonSchemaEditor(true)}
                    variant="outline"
                    size="sm"
                    className="text-xs"
                  >
                    Open Editor
                  </Button>
                </div>

                {field.state.value && field.state.value.trim() && (
                  <div
                    className={`border rounded-lg p-3 bg-background-muted ${
                      field.state.meta.errors.length > 0 ? 'border-red-500 dark:border-red-400' : 'border-border-subtle'
                    }`}
                  >
                    <pre className="text-xs font-mono text-text-default whitespace-pre-wrap break-words max-h-32 overflow-y-auto">
                      {field.state.value}
                    </pre>
                  </div>
                )}

                {field.state.meta.errors.length > 0 && (
                  <p className="text-red-500 dark:text-red-400 text-sm mt-1">{field.state.meta.errors[0]}</p>
                )}

                {/* JSON Schema Editor Modal */}
                <JsonSchemaEditor
                  isOpen={showJsonSchemaEditor}
                  onClose={() => setShowJsonSchemaEditor(false)}
                  value={field.state.value || ''}
                  onChange={(value) => {
                    field.handleChange(value);
                    onJsonSchemaChange?.(value);
                  }}
                  error={
                    field.state.meta.errors.length > 0 ? field.state.meta.errors[0] : undefined
                  }
                />
              </div>
            )}
          </form.Field>

          {/* Model Settings */}
          <div>
            <label className="block text-sm font-medium text-text-default mb-1">
              Model Settings
            </label>
            <p className="text-xs text-text-muted mb-3">
              Override the default LLM for this workflow. Leave blank to use the global default.
            </p>
            <div className="grid grid-cols-2 gap-3">
              <form.Field name="settings.biorouter_provider">
                {(field: FormFieldApi<string | undefined>) => (
                  <div>
                    <label className="block text-xs font-medium text-text-muted uppercase tracking-wider mb-1">
                      Provider
                    </label>
                    <input
                      type="text"
                      value={field.state.value || ''}
                      onChange={(e) => field.handleChange(e.target.value || undefined)}
                      placeholder="e.g. anthropic, openai"
                      className="w-full h-9 px-3 text-sm border border-border-subtle rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150"
                    />
                  </div>
                )}
              </form.Field>
              <form.Field name="settings.biorouter_model">
                {(field: FormFieldApi<string | undefined>) => (
                  <div>
                    <label className="block text-xs font-medium text-text-muted uppercase tracking-wider mb-1">
                      Model
                    </label>
                    <input
                      type="text"
                      value={field.state.value || ''}
                      onChange={(e) => field.handleChange(e.target.value || undefined)}
                      placeholder="e.g. claude-opus-4-5"
                      className="w-full h-9 px-3 text-sm border border-border-subtle rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150"
                    />
                  </div>
                )}
              </form.Field>
            </div>
            <form.Field name="settings.temperature">
              {(field: FormFieldApi<number | undefined>) => (
                <div className="mt-3">
                  <label className="block text-xs font-medium text-text-muted uppercase tracking-wider mb-1">
                    Temperature{' '}
                    <span className="normal-case font-normal">(0–2, blank = default)</span>
                  </label>
                  <input
                    type="number"
                    min={0}
                    max={2}
                    step={0.1}
                    value={field.state.value ?? ''}
                    onChange={(e) => {
                      const v = e.target.value;
                      field.handleChange(v === '' ? undefined : parseFloat(v));
                    }}
                    placeholder="e.g. 0.7"
                    className="w-full h-9 px-3 text-sm border border-border-subtle rounded-lg bg-background-default text-text-default placeholder:text-text-muted focus:outline-none focus:border-border-strong transition-colors duration-150"
                  />
                </div>
              )}
            </form.Field>
          </div>

          {/* Extensions */}
          {onExtensionsChange && (
            <div>
              <label className="block text-sm font-medium text-text-default mb-1">
                Extensions
              </label>
              <p className="text-xs text-text-muted mb-3">
                Select which extensions are active for this workflow. Unchecked extensions are
                disabled regardless of global settings.
              </p>
              {availableExtensions.length === 0 ? (
                <p className="text-xs text-text-muted italic">No extensions configured.</p>
              ) : (
                <div className="space-y-2">
                  {availableExtensions.map((ext) => {
                    const isSelected = extensions.some((e) => e.name === ext.name);
                    return (
                      <label
                        key={ext.name}
                        className="flex items-start gap-3 py-2.5 px-3 rounded-lg border border-border-subtle bg-background-default hover:bg-background-muted cursor-pointer transition-colors duration-150"
                      >
                        <input
                          type="checkbox"
                          checked={isSelected}
                          onChange={(e) => {
                            if (e.target.checked) {
                              onExtensionsChange([...extensions, ext]);
                            } else {
                              onExtensionsChange(extensions.filter((x) => x.name !== ext.name));
                            }
                          }}
                          className="mt-0.5 accent-[#cf6d47] flex-shrink-0"
                        />
                        <div className="min-w-0">
                          <p className="text-sm font-medium text-text-default">{ext.name}</p>
                          {ext.description && (
                            <p className="text-xs text-text-muted mt-0.5 truncate">
                              {ext.description}
                            </p>
                          )}
                        </div>
                      </label>
                    );
                  })}
                </div>
              )}
            </div>
          )}
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}
