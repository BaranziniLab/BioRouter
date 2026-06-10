import { useState, useEffect } from 'react';
import { createPortal } from 'react-dom';
import { useForm } from '@tanstack/react-form';
import { Workflow, WorkflowKnowledgeBases } from '../../workflow';
import { X, Save, Play, Loader2 } from '../icons/app-icons';
import { Button } from '../ui/button';
import { WorkflowFormFields } from './shared/WorkflowFormFields';
import { WorkflowFormData } from './shared/workflowFormSchema';
import { createWorkflow, getActive, getSessionExtensions, listBases } from '../../api/sdk.gen';
import { WorkflowParameter } from './shared/workflowFormSchema';
import { toastError } from '../../toasts';
import { saveWorkflow } from '../../workflow/workflow_management';
import type { ExtensionConfig, Manifest } from '../../api';
import { ALL_SKILL_DIRS, loadSkillsFromDirs } from '../skills/skillUtils';
import type { WorkflowResourceItem } from './shared/WorkflowResourcePicker';

interface CreateWorkflowFromSessionModalProps {
  isOpen: boolean;
  onClose: () => void;
  sessionId: string;
  onWorkflowCreated?: (workflow: Workflow) => void;
}

export default function CreateWorkflowFromSessionModal({
  isOpen,
  onClose,
  sessionId,
  onWorkflowCreated,
}: CreateWorkflowFromSessionModalProps) {
  const [isCreating, setIsCreating] = useState(false);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [analysisStage, setAnalysisStage] = useState<string>('');
  const [hasAnalyzed, setHasAnalyzed] = useState(false);
  const [workflowExtensions, setWorkflowExtensions] = useState<ExtensionConfig[]>([]);
  const [knowledgeBaseItems, setKnowledgeBaseItems] = useState<WorkflowResourceItem[]>([]);
  const [workflowKnowledgeBaseIds, setWorkflowKnowledgeBaseIds] = useState<string[]>([]);
  const [defaultKnowledgeBaseId, setDefaultKnowledgeBaseId] = useState<string | null>(null);
  const [skillItems, setSkillItems] = useState<WorkflowResourceItem[]>([]);
  const [workflowSkillIds, setWorkflowSkillIds] = useState<string[]>([]);

  // Initialize form with empty values for new workflow
  const form = useForm({
    defaultValues: {
      title: '',
      description: '',
      instructions: '',
      prompt: '',
      activities: [] as string[],
      parameters: [] as WorkflowParameter[],
      jsonSchema: '',
      workflowName: '',
      global: true,
    } as WorkflowFormData,
    onSubmit: async ({ value }) => {
      await handleCreateWorkflow(value);
    },
  });

  // Track form validity with state to make it reactive
  const [isFormValid, setIsFormValid] = useState(false);

  // Analyze messages and prefill form when modal opens
  useEffect(() => {
    if (isOpen && sessionId && !hasAnalyzed) {
      setIsAnalyzing(true);

      // Create a sequence of analysis stages for better UX
      const stages = [
        'Reading your conversation...',
        'Identifying key patterns...',
        'Extracting main topics...',
        'Generating workflow structure...',
        'Finalizing details...',
      ];

      let currentStageIndex = 0;
      setAnalysisStage(stages[0]);

      // Update stage every 800ms
      const stageInterval = setInterval(() => {
        currentStageIndex = (currentStageIndex + 1) % stages.length;
        setAnalysisStage(stages[currentStageIndex]);
      }, 800);

      // Pre-select session extensions immediately — independent of workflow analysis
      getSessionExtensions({ path: { session_id: sessionId }, throwOnError: false }).then(
        (res) => {
          if (res.data?.extensions) {
            setWorkflowExtensions(res.data.extensions);
          }
        }
      );

      Promise.all([
        listBases({ throwOnError: false }),
        getActive({ query: { session_id: sessionId }, throwOnError: false }),
      ]).then(([basesRes, activeRes]) => {
        const bases: Manifest[] = basesRes.data ?? [];
        const hidden = new Set(activeRes.data?.hidden_kbs ?? []);
        const visible = bases.filter((base) => !hidden.has(base.id)).map((base) => base.id);
        const defaultId =
          activeRes.data?.active_kb && visible.includes(activeRes.data.active_kb)
            ? activeRes.data.active_kb
            : visible[0] ?? null;

        setKnowledgeBaseItems(
          bases.map((base) => ({
            id: base.id,
            label: base.name,
            description: base.id,
          }))
        );
        setWorkflowKnowledgeBaseIds(visible);
        setDefaultKnowledgeBaseId(defaultId);
      });

      loadSkillsFromDirs(ALL_SKILL_DIRS)
        .then(({ singles, bundles }) => {
          const bundleItems: WorkflowResourceItem[] = bundles.map((bundle) => ({
            id: bundle.bundleName,
            label: bundle.bundleName,
            description: bundle.skills.map((skill) => skill.name).join(', '),
            badge: 'bundle',
          }));
          const singleItems: WorkflowResourceItem[] = singles.map((skill) => ({
            id: skill.name,
            label: skill.name,
            description: skill.description,
          }));
          setSkillItems([...bundleItems, ...singleItems]);
        })
        .catch((error) => {
          console.warn('Failed to load workflow skills:', error);
          setSkillItems([]);
        });

      // Analyze the conversation to generate a suggested workflow
      createWorkflow({
        body: { session_id: sessionId },
        throwOnError: true,
      })
        .then((response) => {
          clearInterval(stageInterval);
          setAnalysisStage('Complete!');

          if (response.data?.workflow) {
            const workflow = response.data.workflow;

            // Prefill the form with the analyzed workflow information
            form.setFieldValue('title', workflow.title || '');
            form.setFieldValue('description', workflow.description || '');
            form.setFieldValue('instructions', workflow.instructions || '');
            form.setFieldValue('activities', workflow.activities || []);
            form.setFieldValue('parameters', workflow.parameters || []);

            if (workflow.response?.json_schema) {
              form.setFieldValue(
                'jsonSchema',
                JSON.stringify(workflow.response.json_schema, null, 2)
              );
            }
          } else {
            console.error('No workflow in response:', response);
          }
          setHasAnalyzed(true);
        })
        .catch((error) => {
          console.error('Failed to analyze messages:', error);
          setAnalysisStage('Analysis failed');
        })
        .finally(() => {
          clearInterval(stageInterval);
          setHasAnalyzed(true);
          setTimeout(() => {
            setIsAnalyzing(false);
            setAnalysisStage('');
          }, 500);
        });
    }
  }, [isOpen, sessionId, hasAnalyzed, form]);

  // Reset state when modal closes
  useEffect(() => {
    if (!isOpen) {
      setHasAnalyzed(false);
      setIsAnalyzing(false);
      setAnalysisStage('');
      setWorkflowExtensions([]);
      setKnowledgeBaseItems([]);
      setWorkflowKnowledgeBaseIds([]);
      setDefaultKnowledgeBaseId(null);
      setSkillItems([]);
      setWorkflowSkillIds([]);
    }
  }, [isOpen]);

  // Subscribe to form changes using the form's subscribe method
  useEffect(() => {
    const unsubscribe = form.store.subscribe(() => {
      const hasTitle = form.state.values.title?.trim();
      const hasDescription = form.state.values.description?.trim();
      const hasInstructions = form.state.values.instructions?.trim();
      const valid = !!(hasTitle && hasDescription && hasInstructions);

      setIsFormValid(valid);
    });

    // Initial validation check
    const hasTitle = form.state.values.title?.trim();
    const hasDescription = form.state.values.description?.trim();
    const hasInstructions = form.state.values.instructions?.trim();
    const valid = !!(hasTitle && hasDescription && hasInstructions);
    setIsFormValid(valid);

    return unsubscribe;
  }, [form]);

  const handleCreateWorkflow = async (formData: WorkflowFormData, runAfterSave = false) => {
    if (!isFormValid) {
      return;
    }

    setIsCreating(true);
    try {
      // Build settings if any model override was specified
      const settingsConfig =
        formData.settings?.biorouter_provider ||
        formData.settings?.biorouter_model ||
        formData.settings?.temperature !== undefined
          ? {
              biorouter_provider: formData.settings?.biorouter_provider || undefined,
              biorouter_model: formData.settings?.biorouter_model || undefined,
              temperature: formData.settings?.temperature,
            }
          : undefined;

      // Create the workflow object from form data
      const knowledgeBases: WorkflowKnowledgeBases | undefined =
        knowledgeBaseItems.length > 0
          ? {
              default: defaultKnowledgeBaseId,
              visible: workflowKnowledgeBaseIds,
            }
          : undefined;

      const workflow: Workflow = {
        title: formData.title,
        description: formData.description,
        instructions: formData.instructions,
        prompt: formData.prompt || undefined,
        activities: formData.activities.filter((activity) => activity.trim() !== ''),
        parameters: formData.parameters.map((param) => ({
          key: param.key,
          input_type: param.input_type || 'string',
          requirement: param.requirement,
          description: param.description,
          ...(param.requirement === 'optional' && param.default ? { default: param.default } : {}),
          ...(param.input_type === 'select' && param.options
            ? {
                options: param.options.filter((opt: string) => opt.trim() !== ''),
              }
            : {}),
        })),
        response:
          formData.jsonSchema && formData.jsonSchema.trim()
            ? {
                json_schema: JSON.parse(formData.jsonSchema),
              }
            : undefined,
        settings: settingsConfig,
        extensions: workflowExtensions.length > 0
          ? workflowExtensions.map((ext) =>
              'envs' in ext ? { ...ext, envs: undefined } : ext
            ) as ExtensionConfig[]
          : undefined,
        knowledge_bases: knowledgeBases,
        skills: workflowSkillIds.length > 0 ? workflowSkillIds : undefined,
      };

      let workflowId = await saveWorkflow(workflow, null);

      onWorkflowCreated?.(workflow);
      onClose();

      if (runAfterSave) {
        window.electron.createChatWindow(
          undefined,
          undefined,
          undefined,
          undefined,
          undefined,
          workflowId
        );
      }
    } catch (error) {
      console.error('Failed to create workflow:', error);
      toastError({
        title: 'Failed to create workflow',
        msg:
          error instanceof Error
            ? error.message
            : 'An unexpected error occurred while creating the workflow. Please try again.',
      });
    } finally {
      setIsCreating(false);
    }
  };

  // ESC-to-close — survives tiny chat windows in dashboard mode where the
  // in-modal close button can be off-screen.
  useEffect(() => {
    if (!isOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  // Portal to document.body so the fixed overlay anchors to the viewport
  // instead of the dashboard's transformed world layer (translate3d on
  // an ancestor turns `fixed` into a constrained containing block).
  return createPortal(
    <div
      className="fixed inset-0 z-[400] flex items-center justify-center bg-black/50 p-4"
      data-testid="create-workflow-modal"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="flex h-full max-h-[90vh] w-full max-w-4xl flex-col rounded-xl bg-background-default shadow-2xl">
        {/* Header */}
        <div
          className="flex shrink-0 items-center justify-between px-6 pb-3 pt-5"
          data-testid="modal-header"
        >
          <div>
            <h2 className="text-base font-semibold text-text-default">
              Create Workflow from Session
            </h2>
            <p className="text-xs text-text-muted mt-0.5">
              Create a reusable workflow based on your current conversation
            </p>
          </div>
          <Button
            onClick={onClose}
            variant="ghost"
            size="sm"
            className="rounded-md p-1.5 transition-colors hover:bg-background-medium"
            data-testid="close-button"
          >
            <X className="w-4 h-4" />
          </Button>
        </div>

        {/* Content */}
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-4" data-testid="modal-content">
          {isAnalyzing ? (
            <div
              className="flex flex-col items-center justify-center h-full min-h-[300px] gap-3"
              data-testid="analyzing-state"
            >
              <Loader2
                className="w-6 h-6 animate-spin text-text-muted"
                data-testid="analysis-spinner"
              />
              <div className="text-center">
                <p
                  className="text-sm font-medium text-text-default"
                  data-testid="analyzing-title"
                >
                  Analyzing your conversation
                </p>
                <p className="text-xs text-text-muted mt-1" data-testid="analysis-stage">
                  {analysisStage}
                </p>
              </div>
            </div>
          ) : (
            <div data-testid="form-state">
              <WorkflowFormFields
                form={form}
                extensions={workflowExtensions}
                onExtensionsChange={setWorkflowExtensions}
                knowledgeBaseItems={knowledgeBaseItems}
                selectedKnowledgeBaseIds={workflowKnowledgeBaseIds}
                onKnowledgeBaseIdsChange={(ids) => {
                  setWorkflowKnowledgeBaseIds(ids);
                  if (defaultKnowledgeBaseId && !ids.includes(defaultKnowledgeBaseId)) {
                    setDefaultKnowledgeBaseId(ids[0] ?? null);
                  } else if (!defaultKnowledgeBaseId && ids.length > 0) {
                    setDefaultKnowledgeBaseId(ids[0]);
                  }
                }}
                defaultKnowledgeBaseId={defaultKnowledgeBaseId}
                onDefaultKnowledgeBaseIdChange={setDefaultKnowledgeBaseId}
                skillItems={skillItems}
                selectedSkillIds={workflowSkillIds}
                onSkillIdsChange={setWorkflowSkillIds}
              />
            </div>
          )}
        </div>

        {/* Footer */}
        <div
          className="flex shrink-0 items-center justify-between px-6 pb-5 pt-3"
          data-testid="modal-footer"
        >
          <Button
            onClick={onClose}
            variant="ghost"
            className="rounded-md px-4 py-2 text-text-muted transition-colors hover:bg-background-medium"
            data-testid="cancel-button"
          >
            Cancel
          </Button>

          {!isAnalyzing && (
            <div className="flex gap-3">
              <Button
                onClick={() => form.handleSubmit()}
                disabled={!isFormValid || isCreating}
                variant="ghost"
                size="default"
                className="inline-flex items-center justify-center gap-2 rounded-md bg-background-medium px-4 py-2 hover:bg-background-muted"
                data-testid="create-workflow-button"
              >
                <Save className="w-4 h-4" />
                {isCreating ? 'Creating…' : 'Create Workflow'}
              </Button>
              <Button
                onClick={() => handleCreateWorkflow(form.state.values, true)}
                disabled={!isFormValid || isCreating}
                variant="default"
                size="default"
                className="inline-flex items-center justify-center gap-2 px-4 py-2"
                data-testid="create-and-run-workflow-button"
              >
                <Play className="w-4 h-4" />
                {isCreating ? 'Creating…' : 'Create & Run'}
              </Button>
            </div>
          )}
        </div>
      </div>
    </div>,
    document.body
  );
}
