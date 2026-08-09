import { useState, useEffect, useMemo } from 'react';
import { listSavedWorkflows, convertToLocaleDateString } from '../../workflow/workflow_management';
import {
  Edit,
  Trash2,
  Play,
  Calendar,
  AlertCircle,
  Link,
  Clock,
  Terminal,
  NewWindow,
  Share2,
  Copy,
  Download,
} from '../icons/app-icons';
import { ENTITY_ICONS } from '../icons/entity-icons';
import { ScrollArea } from '../ui/scroll-area';
import { Button } from '../ui/button';
import { Skeleton } from '../ui/skeleton';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { toastSuccess, toastError } from '../../toasts';
import {
  deleteWorkflow,
  WorkflowManifest,
  startAgent,
  scheduleWorkflow,
  setWorkflowSlashCommand,
  workflowToYaml,
} from '../../api';
import ImportWorkflowForm, { ImportWorkflowButton } from './ImportWorkflowForm';
import CreateEditWorkflowModal from './CreateEditWorkflowModal';
import { generateDeepLink, Workflow } from '../../workflow';
import { useNavigation } from '../../hooks/useNavigation';
import { CronPicker } from '../schedule/CronPicker';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '../ui/dialog';
import { SearchView } from '../conversation/SearchView';
import cronstrue from 'cronstrue';
import { getInitialWorkingDir } from '../../utils/workingDir';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
} from '../ui/dropdown-menu';
import { getSearchShortcutText } from '../../utils/keyboardShortcuts';
import { ReadableContent } from '../Layout/ReadableContent';
import BuiltInBadge from '../ui/BuiltInBadge';
import { BUILTIN_RECREATED_TITLE, isBuiltinWorkflow } from '../../utils/builtins';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { EmptyState } from '../ui/empty-state';
import { useConfig } from '../ConfigContext';

const WorkflowIcon = ENTITY_ICONS.workflow;

export default function WorkflowsView() {
  const setView = useNavigation();
  const { refreshConfig } = useConfig();
  const [savedWorkflows, setSavedWorkflows] = useState<WorkflowManifest[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedWorkflow, setSelectedWorkflow] = useState<WorkflowManifest | null>(null);
  const [showEditor, setShowEditor] = useState(false);
  const [workflowToDelete, setWorkflowToDelete] = useState<WorkflowManifest | null>(null);
  const [isDeletingWorkflow, setIsDeletingWorkflow] = useState(false);

  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [showImportDialog, setShowImportDialog] = useState(false);

  const [showScheduleDialog, setShowScheduleDialog] = useState(false);
  const [scheduleWorkflowManifest, setScheduleWorkflowManifest] = useState<WorkflowManifest | null>(
    null
  );
  const [scheduleCron, setScheduleCron] = useState<string>('');
  const [isSavingSchedule, setIsSavingSchedule] = useState(false);

  const [showSlashCommandDialog, setShowSlashCommandDialog] = useState(false);
  const [slashCommandWorkflowManifest, setSlashCommandWorkflowManifest] =
    useState<WorkflowManifest | null>(null);
  const [slashCommand, setSlashCommand] = useState<string>('');
  const [isSavingSlashCommand, setIsSavingSlashCommand] = useState(false);
  const [scheduleValid, setScheduleIsValid] = useState(true);

  const [searchTerm, setSearchTerm] = useState('');

  const filteredWorkflows = useMemo(() => {
    if (!searchTerm) return savedWorkflows;

    const searchLower = searchTerm.toLowerCase();
    return savedWorkflows.filter((workflowManifest) => {
      const { workflow, slash_command } = workflowManifest;
      const title = workflow.title?.toLowerCase() || '';
      const description = workflow.description?.toLowerCase() || '';
      const slashCmd = slash_command?.toLowerCase() || '';

      return (
        title.includes(searchLower) ||
        description.includes(searchLower) ||
        slashCmd.includes(searchLower)
      );
    });
  }, [savedWorkflows, searchTerm]);

  useEffect(() => {
    loadSavedWorkflows();
  }, []);

  const loadSavedWorkflows = async () => {
    try {
      setLoading(true);
      setError(null);
      const workflowManifestResponses = await listSavedWorkflows();
      setSavedWorkflows(workflowManifestResponses);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load workflows');
      console.error('Failed to load saved workflows:', err);
    } finally {
      setLoading(false);
    }
  };

  const handleStartWorkflowChat = async (workflow: Workflow, _workflowId: string) => {
    try {
      const newAgent = await startAgent({
        body: {
          working_dir: getInitialWorkingDir(),
          workflow,
        },
        throwOnError: true,
      });
      const session = newAgent.data;
      setView('pair', {
        disableAnimation: true,
        resumeSessionId: session.id,
      });
    } catch (error) {
      console.error('Failed to load workflow:', error);
      const errorMsg = error instanceof Error ? error.message : 'Failed to load workflow';
      setError(errorMsg);
    }
  };

  const handleStartWorkflowChatInNewWindow = (workflowId: string) => {
    try {
      window.electron.createChatWindow(
        undefined,
        getInitialWorkingDir(),
        undefined,
        undefined,
        'pair',
        workflowId
      );
    } catch (error) {
      console.error('Failed to open workflow in new window:', error);
    }
  };

  const handleDeleteWorkflow = async () => {
    if (!workflowToDelete || isDeletingWorkflow) return;
    const workflowManifest = workflowToDelete;
    setIsDeletingWorkflow(true);
    try {
      await deleteWorkflow({ body: { id: workflowManifest.id } });
      await loadSavedWorkflows();
      setWorkflowToDelete(null);
      toastSuccess({
        title: workflowManifest.workflow.title,
        msg: 'Workflow deleted',
      });
    } catch (err) {
      console.error('Failed to delete workflow:', err);
      const errorMsg = err instanceof Error ? err.message : 'Failed to delete workflow';
      setError(errorMsg);
    } finally {
      setIsDeletingWorkflow(false);
    }
  };

  const handleEditWorkflow = async (workflowManifest: WorkflowManifest) => {
    setSelectedWorkflow(workflowManifest);
    setShowEditor(true);
  };

  const handleEditorClose = (wasSaved?: boolean) => {
    setShowEditor(false);
    setSelectedWorkflow(null);
    if (wasSaved) {
      loadSavedWorkflows();
    }
  };

  const handleCopyDeeplink = async (workflowManifest: WorkflowManifest) => {
    try {
      const deeplink = await generateDeepLink(workflowManifest.workflow);
      await navigator.clipboard.writeText(deeplink);
      toastSuccess({
        title: 'Deeplink copied',
        msg: 'Workflow deeplink has been copied to clipboard',
      });
    } catch (error) {
      console.error('Failed to copy deeplink:', error);
      toastError({
        title: 'Copy failed',
        msg: 'Failed to copy deeplink to clipboard',
      });
    }
  };

  const handleCopyYaml = async (workflowManifest: WorkflowManifest) => {
    try {
      const response = await workflowToYaml({
        body: { workflow: workflowManifest.workflow },
        throwOnError: true,
      });

      if (!response.data?.yaml) {
        throw new Error('No YAML data returned from API');
      }

      await navigator.clipboard.writeText(response.data.yaml);
      toastSuccess({
        title: 'YAML copied',
        msg: 'Workflow YAML has been copied to clipboard',
      });
    } catch (error) {
      console.error('Failed to copy YAML:', error);
      toastError({
        title: 'Copy failed',
        msg: 'Failed to copy workflow YAML to clipboard',
      });
    }
  };

  const handleExportFile = async (workflowManifest: WorkflowManifest) => {
    try {
      const response = await workflowToYaml({
        body: { workflow: workflowManifest.workflow },
        throwOnError: true,
      });

      if (!response.data?.yaml) {
        throw new Error('No YAML data returned from API');
      }

      const sanitizedTitle = (workflowManifest.workflow.title || 'workflow')
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-|-$/g, '');

      const filename = `${sanitizedTitle}.yaml`;

      const result = await window.electron.showSaveDialog({
        title: 'Export workflow',
        defaultPath: filename,
        filters: [
          { name: 'YAML Files', extensions: ['yaml', 'yml'] },
          { name: 'All Files', extensions: ['*'] },
        ],
      });

      if (!result.canceled && result.filePath) {
        await window.electron.writeFile(result.filePath, response.data.yaml);
        toastSuccess({
          title: 'Workflow exported',
          msg: `Workflow saved to ${result.filePath}`,
        });
      }
    } catch (error) {
      console.error('Failed to export workflow:', error);
      toastError({
        title: 'Export failed',
        msg: 'Failed to export workflow to file',
      });
    }
  };

  const handleOpenScheduleDialog = (workflowManifest: WorkflowManifest) => {
    setScheduleWorkflowManifest(workflowManifest);
    setScheduleCron(workflowManifest.schedule_cron || '0 0 14 * * *');
    setShowScheduleDialog(true);
  };

  const handleSaveSchedule = async () => {
    if (!scheduleWorkflowManifest || isSavingSchedule) return;

    setIsSavingSchedule(true);
    try {
      await scheduleWorkflow({
        body: {
          id: scheduleWorkflowManifest.id,
          cron_schedule: scheduleCron,
        },
      });

      toastSuccess({
        title: 'Schedule saved',
        msg: `Workflow will run ${getReadableCron(scheduleCron)}`,
      });

      setShowScheduleDialog(false);
      setScheduleWorkflowManifest(null);
      await loadSavedWorkflows();
    } catch (error) {
      console.error('Failed to save schedule:', error);
      const errorMsg = error instanceof Error ? error.message : 'Failed to save schedule';
      setError(errorMsg);
    } finally {
      setIsSavingSchedule(false);
    }
  };

  const handleRemoveSchedule = async () => {
    if (!scheduleWorkflowManifest || isSavingSchedule) return;

    setIsSavingSchedule(true);
    try {
      await scheduleWorkflow({
        body: {
          id: scheduleWorkflowManifest.id,
          cron_schedule: null,
        },
      });

      toastSuccess({
        title: 'Schedule removed',
        msg: 'Workflow will no longer run automatically',
      });

      setShowScheduleDialog(false);
      setScheduleWorkflowManifest(null);
      await loadSavedWorkflows();
    } catch (error) {
      console.error('Failed to remove schedule:', error);
      const errorMsg = error instanceof Error ? error.message : 'Failed to remove schedule';
      setError(errorMsg);
    } finally {
      setIsSavingSchedule(false);
    }
  };

  const handleOpenSlashCommandDialog = (workflowManifest: WorkflowManifest) => {
    setSlashCommandWorkflowManifest(workflowManifest);
    setSlashCommand(workflowManifest.slash_command || '');
    setShowSlashCommandDialog(true);
  };

  const refreshConfigAfterSlashCommandWrite = async () => {
    try {
      await refreshConfig();
    } catch (error) {
      console.error('Failed to refresh config after updating a workflow slash command:', error);
    }
  };

  const handleSaveSlashCommand = async () => {
    if (!slashCommandWorkflowManifest || isSavingSlashCommand) return;

    setIsSavingSlashCommand(true);
    try {
      await setWorkflowSlashCommand({
        body: {
          id: slashCommandWorkflowManifest.id,
          slash_command: slashCommand || null,
        },
      });
      await refreshConfigAfterSlashCommandWrite();

      toastSuccess({
        title: 'Slash command saved',
        msg: slashCommand ? `Use /${slashCommand} to run this workflow` : 'Slash command removed',
      });

      setShowSlashCommandDialog(false);
      setSlashCommandWorkflowManifest(null);
      await loadSavedWorkflows();
    } catch (error) {
      console.error('Failed to save slash command:', error);
      const errorMsg = error instanceof Error ? error.message : 'Failed to save slash command';
      setError(errorMsg);
    } finally {
      setIsSavingSlashCommand(false);
    }
  };

  const handleRemoveSlashCommand = async () => {
    if (!slashCommandWorkflowManifest || isSavingSlashCommand) return;

    setIsSavingSlashCommand(true);
    try {
      await setWorkflowSlashCommand({
        body: {
          id: slashCommandWorkflowManifest.id,
          slash_command: null,
        },
      });
      await refreshConfigAfterSlashCommandWrite();

      toastSuccess({
        title: 'Slash command removed',
        msg: 'Workflow slash command has been removed',
      });

      setShowSlashCommandDialog(false);
      setSlashCommandWorkflowManifest(null);
      await loadSavedWorkflows();
    } catch (error) {
      console.error('Failed to remove slash command:', error);
      const errorMsg = error instanceof Error ? error.message : 'Failed to remove slash command';
      setError(errorMsg);
    } finally {
      setIsSavingSlashCommand(false);
    }
  };

  const getReadableCron = (cron: string): string => {
    try {
      const cronWithoutSeconds = cron.split(' ').slice(1).join(' ');
      return cronstrue.toString(cronWithoutSeconds).toLowerCase();
    } catch {
      return cron;
    }
  };

  const WorkflowItem = ({
    workflowManifestResponse,
    workflowManifestResponse: {
      workflow,
      last_modified: lastModified,
      schedule_cron,
      slash_command,
    },
  }: {
    workflowManifestResponse: WorkflowManifest;
  }) => (
    <div className="biorouter-list-row py-3 px-3 group">
      <div className="flex justify-between items-start gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <h3 className="text-label text-text-default truncate max-w-[50vw]">{workflow.title}</h3>
            {isBuiltinWorkflow(workflowManifestResponse.file_path) && (
              <BuiltInBadge title={BUILTIN_RECREATED_TITLE} />
            )}
          </div>
          <p className="text-supporting text-text-muted mt-0.5 line-clamp-1">
            {workflow.description}
          </p>
          <div className="flex items-center gap-3 mt-1 text-supporting text-text-subtle">
            <span className="flex items-center">
              <Calendar className="w-3 h-3 mr-1" />
              {convertToLocaleDateString(lastModified)}
            </span>
            {schedule_cron && (
              <span className="flex items-center text-text-info">
                <Clock className="w-3 h-3 mr-1" />
                Runs {getReadableCron(schedule_cron)}
              </span>
            )}
            {slash_command && (
              <span className="flex items-center text-text-info">/{slash_command}</span>
            )}
          </div>
        </div>

        <div className="flex items-center gap-1 shrink-0 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
          <Button
            onClick={(e) => {
              e.stopPropagation();
              handleOpenSlashCommandDialog(workflowManifestResponse);
            }}
            variant="ghost"
            shape="round"
            /*
             * ⚠ State, not emphasis. This was `default` when a slash command
             * exists, so the state was drawn as a solid accent fill. Flattening
             * every row action to ghost would have DELETED that signal, which is
             * a functional regression rather than a style change.
             *
             * `tint-selected` is the system's answer for exactly this case:
             * selection is a state the app sets, not a user interaction. Paired
             * with `tint-interactive` because hover alone (5%) is lighter than
             * the selected wash (14%) and would visibly un-select on hover.
             */
            className={slash_command ? 'tint-selected tint-interactive' : undefined}
            title={slash_command ? 'Edit slash command' : 'Add slash command'}
          >
            <Terminal className="w-4 h-4" />
          </Button>
          <Button
            onClick={(e) => {
              e.stopPropagation();
              handleStartWorkflowChat(workflow, workflowManifestResponse.id);
            }}
            shape="round"
            title="Use workflow"
          >
            <Play className="w-4 h-4" />
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                onClick={(e) => e.stopPropagation()}
                variant="ghost"
                shape="round"
                title="Launch workflow"
              >
                <NewWindow className="w-4 h-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
              <DropdownMenuItem
                onClick={() => handleStartWorkflowChatInNewWindow(workflowManifestResponse.id)}
              >
                <NewWindow className="w-4 h-4" />
                Open in new window
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <Button
            onClick={(e) => {
              e.stopPropagation();
              handleEditWorkflow(workflowManifestResponse);
            }}
            variant="ghost"
            shape="round"
            title="Edit workflow"
          >
            <Edit className="w-4 h-4" />
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                onClick={(e) => e.stopPropagation()}
                variant="ghost"
                shape="round"
                title="Share workflow"
              >
                <Share2 className="w-4 h-4" />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" onClick={(e) => e.stopPropagation()}>
              <DropdownMenuItem onClick={() => handleCopyDeeplink(workflowManifestResponse)}>
                <Link className="w-4 h-4" />
                Copy Deeplink
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => handleCopyYaml(workflowManifestResponse)}>
                <Copy className="w-4 h-4" />
                Copy YAML
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem onClick={() => handleExportFile(workflowManifestResponse)}>
                <Download className="w-4 h-4" />
                Export to File
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          <Button
            onClick={(e) => {
              e.stopPropagation();
              handleOpenScheduleDialog(workflowManifestResponse);
            }}
            variant="ghost"
            shape="round"
            // Same state-as-tint treatment as the slash-command button above.
            className={schedule_cron ? 'tint-selected tint-interactive' : undefined}
            title={schedule_cron ? 'Edit schedule' : 'Add schedule'}
          >
            <Clock className="w-4 h-4" />
          </Button>
          <Button
            onClick={(e) => {
              e.stopPropagation();
              setWorkflowToDelete(workflowManifestResponse);
            }}
            variant="ghost"
            size="sm"
            className="text-text-danger"
            title="Delete workflow"
          >
            <Trash2 className="w-4 h-4" />
          </Button>
        </div>
      </div>
    </div>
  );

  const WorkflowSkeleton = () => (
    <div className="biorouter-list-row py-4 px-3">
      <div className="flex justify-between items-start gap-4">
        <div className="min-w-0 flex-1">
          <Skeleton className="h-5 w-3/4 mb-2" />
          <Skeleton className="h-4 w-full mb-2" />
          <Skeleton className="h-4 w-24" />
        </div>
        <div className="flex items-center gap-2 shrink-0">
          <Skeleton className="h-8 w-8" />
          <Skeleton className="h-8 w-8" />
          <Skeleton className="h-8 w-8" />
          <Skeleton className="h-8 w-8" />
          <Skeleton className="h-8 w-8" />
        </div>
      </div>
    </div>
  );

  const renderContent = () => {
    if (loading) {
      return (
        <div className="space-y-6">
          <div className="space-y-3">
            <Skeleton className="h-6 w-24" />
            <div className="space-y-2">
              <WorkflowSkeleton />
              <WorkflowSkeleton />
              <WorkflowSkeleton />
            </div>
          </div>
        </div>
      );
    }

    if (error) {
      return (
        <div className="flex flex-col items-center justify-center h-full text-text-muted">
          <AlertCircle className="h-12 w-12 text-text-danger mb-4" />
          <p className="text-subheading mb-2">Error Loading Workflows</p>
          <p className="text-body text-center mb-4">{error}</p>
          <Button onClick={loadSavedWorkflows} variant="default">
            Try Again
          </Button>
        </div>
      );
    }

    if (savedWorkflows.length === 0) {
      return (
        <EmptyState
          icon={WorkflowIcon}
          title="No workflows yet"
          description="Create a reusable workflow here, save one from a chat, or import an existing workflow."
          actions={
            <>
              <Button onClick={() => setShowCreateDialog(true)}>
                <WorkflowIcon className="h-4 w-4" />
                Create workflow
              </Button>
              <Button onClick={() => setShowImportDialog(true)} variant="outline">
                Import workflow
              </Button>
            </>
          }
        />
      );
    }

    if (filteredWorkflows.length === 0 && searchTerm) {
      return (
        <EmptyState
          icon={WorkflowIcon}
          title="No matching workflows"
          description="Try a different title, description, or slash command."
          compact
        />
      );
    }

    return (
      <div className="biorouter-list-shell">
        {filteredWorkflows.map((workflowManifestResponse: WorkflowManifest) => (
          <WorkflowItem
            key={workflowManifestResponse.id}
            workflowManifestResponse={workflowManifestResponse}
          />
        ))}
      </div>
    );
  };

  return (
    <>
      <MainPanelLayout>
        <div className="flex-1 flex flex-col min-h-0">
          {/* Flat page header */}
          <div className="flex-shrink-0 border-b border-border-subtle">
            <ReadableContent className="px-8 pt-12 pb-6">
              <h1 className="text-title mb-1 page-transition">Workflows</h1>
              <p className="text-body text-text-muted mb-0">
                View and manage your saved workflows to quickly start new chats with predefined
                configurations. {getSearchShortcutText()} to search.
              </p>
              <div className="flex gap-3 mt-5">
                <Button
                  onClick={() => setShowCreateDialog(true)}
                  variant="default"
                  className="flex items-center gap-2"
                >
                  <WorkflowIcon className="w-4 h-4" />
                  Create Workflow
                </Button>
                <ImportWorkflowButton onClick={() => setShowImportDialog(true)} />
              </div>
            </ReadableContent>
          </div>

          <ReadableContent className="flex-1 min-h-0 relative px-8 pt-6">
            <ScrollArea className="h-full">
              <SearchView
                onSearch={(term) => setSearchTerm(term)}
                placeholder="Search workflows..."
              >
                <div className="h-full relative">{renderContent()}</div>
              </SearchView>
            </ScrollArea>
          </ReadableContent>
        </div>
      </MainPanelLayout>

      {showEditor && selectedWorkflow && (
        <CreateEditWorkflowModal
          isOpen={showEditor}
          onClose={handleEditorClose}
          workflow={selectedWorkflow.workflow}
          workflowId={selectedWorkflow.id}
        />
      )}

      <ImportWorkflowForm
        isOpen={showImportDialog}
        onClose={() => setShowImportDialog(false)}
        onSuccess={loadSavedWorkflows}
      />

      {showCreateDialog && (
        <CreateEditWorkflowModal
          isOpen={showCreateDialog}
          onClose={() => {
            setShowCreateDialog(false);
            loadSavedWorkflows();
          }}
          isCreateMode={true}
        />
      )}

      {showScheduleDialog && scheduleWorkflowManifest && (
        <Dialog
          open={showScheduleDialog}
          onOpenChange={(open) => !isSavingSchedule && setShowScheduleDialog(open)}
        >
          <DialogContent dismissible={!isSavingSchedule} className="max-w-md">
            <DialogHeader>
              <DialogTitle>
                {scheduleWorkflowManifest.schedule_cron ? 'Edit' : 'Add'} Schedule
              </DialogTitle>
            </DialogHeader>
            <div className="space-y-4">
              <CronPicker
                schedule={
                  scheduleWorkflowManifest.schedule_cron
                    ? {
                        id: scheduleWorkflowManifest.id,
                        source: '',
                        cron: scheduleWorkflowManifest.schedule_cron,
                        last_run: null,
                        currently_running: false,
                        paused: false,
                      }
                    : null
                }
                onChange={setScheduleCron}
                isValid={setScheduleIsValid}
              />
              <div className="flex gap-2 justify-end">
                {scheduleWorkflowManifest.schedule_cron && (
                  <Button
                    variant="outline"
                    onClick={handleRemoveSchedule}
                    disabled={isSavingSchedule}
                  >
                    {isSavingSchedule ? 'Working…' : 'Remove Schedule'}
                  </Button>
                )}
                <Button
                  variant="outline"
                  onClick={() => setShowScheduleDialog(false)}
                  disabled={isSavingSchedule}
                >
                  Cancel
                </Button>
                <Button onClick={handleSaveSchedule} disabled={!scheduleValid || isSavingSchedule}>
                  {isSavingSchedule ? 'Saving…' : 'Save'}
                </Button>
              </div>
            </div>
          </DialogContent>
        </Dialog>
      )}

      {showSlashCommandDialog && slashCommandWorkflowManifest && (
        <Dialog
          open={showSlashCommandDialog}
          onOpenChange={(open) => !isSavingSlashCommand && setShowSlashCommandDialog(open)}
        >
          <DialogContent dismissible={!isSavingSlashCommand} className="max-w-md">
            <DialogHeader>
              <DialogTitle>Slash Command</DialogTitle>
            </DialogHeader>
            <div className="space-y-4">
              <div>
                <p className="text-body text-text-muted mb-3">
                  Set a slash command to quickly run this workflow from any chat
                </p>
                <div className="flex gap-2 items-center">
                  <span className="text-text-muted">/</span>
                  <input
                    type="text"
                    value={slashCommand}
                    onChange={(e) => setSlashCommand(e.target.value)}
                    placeholder="command-name"
                    className="flex-1 px-3 py-2 border border-border-subtle rounded-element text-body"
                  />
                </div>
                {slashCommand && (
                  <p className="text-supporting text-text-muted mt-2">
                    Use /{slashCommand} in any chat to run this workflow
                  </p>
                )}
              </div>

              <div className="flex gap-2 justify-end">
                {slashCommandWorkflowManifest.slash_command && (
                  <Button
                    variant="outline"
                    onClick={handleRemoveSlashCommand}
                    disabled={isSavingSlashCommand}
                  >
                    {isSavingSlashCommand ? 'Working…' : 'Remove'}
                  </Button>
                )}
                <Button
                  variant="outline"
                  onClick={() => setShowSlashCommandDialog(false)}
                  disabled={isSavingSlashCommand}
                >
                  Cancel
                </Button>
                <Button onClick={handleSaveSlashCommand} disabled={isSavingSlashCommand}>
                  {isSavingSlashCommand ? 'Saving…' : 'Save'}
                </Button>
              </div>
            </div>
          </DialogContent>
        </Dialog>
      )}

      <ConfirmationModal
        isOpen={workflowToDelete !== null}
        title={`Delete "${workflowToDelete?.workflow.title ?? ''}"?`}
        message="This permanently removes the workflow file. This action cannot be undone."
        confirmLabel="Delete"
        cancelLabel="Cancel"
        confirmVariant="destructive"
        isSubmitting={isDeletingWorkflow}
        onConfirm={() => void handleDeleteWorkflow()}
        onCancel={() => setWorkflowToDelete(null)}
      />
    </>
  );
}
