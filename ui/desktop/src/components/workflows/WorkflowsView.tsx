import { useState, useEffect, useMemo } from 'react';
import { listSavedWorkflows, convertToLocaleDateString } from '../../workflow/workflow_management';
import {
  FileText,
  Edit,
  Trash2,
  Play,
  Calendar,
  AlertCircle,
  Link,
  Clock,
  Terminal,
  ExternalLink,
  Share2,
  Copy,
  Download,
} from 'lucide-react';
import { ScrollArea } from '../ui/scroll-area';
import { Button } from '../ui/button';
import { Skeleton } from '../ui/skeleton';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { toastSuccess, toastError } from '../../toasts';
import { useEscapeKey } from '../../hooks/useEscapeKey';
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
  trackWorkflowDeleted,
  trackWorkflowStarted,
  trackWorkflowDeeplinkCopied,
  trackWorkflowYamlCopied,
  trackWorkflowExportedToFile,
  trackWorkflowScheduled,
  trackWorkflowSlashCommandSet,
  getErrorType,
} from '../../utils/analytics';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  DropdownMenuSeparator,
} from '../ui/dropdown-menu';
import { getSearchShortcutText } from '../../utils/keyboardShortcuts';

export default function WorkflowsView() {
  const setView = useNavigation();
  const [savedWorkflows, setSavedWorkflows] = useState<WorkflowManifest[]>([]);
  const [loading, setLoading] = useState(true);
  const [showSkeleton, setShowSkeleton] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedWorkflow, setSelectedWorkflow] = useState<WorkflowManifest | null>(null);
  const [showEditor, setShowEditor] = useState(false);
  const [showContent, setShowContent] = useState(false);

  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [showImportDialog, setShowImportDialog] = useState(false);

  const [showScheduleDialog, setShowScheduleDialog] = useState(false);
  const [scheduleWorkflowManifest, setScheduleWorkflowManifest] = useState<WorkflowManifest | null>(null);
  const [scheduleCron, setScheduleCron] = useState<string>('');

  const [showSlashCommandDialog, setShowSlashCommandDialog] = useState(false);
  const [slashCommandWorkflowManifest, setSlashCommandWorkflowManifest] =
    useState<WorkflowManifest | null>(null);
  const [slashCommand, setSlashCommand] = useState<string>('');
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

  useEscapeKey(showEditor, () => setShowEditor(false));

  useEffect(() => {
    if (!loading && showSkeleton) {
      const timer = setTimeout(() => {
        setShowSkeleton(false);
        setTimeout(() => {
          setShowContent(true);
        }, 50);
      }, 300);

      return () => clearTimeout(timer);
    }
    return () => void 0;
  }, [loading, showSkeleton]);

  const loadSavedWorkflows = async () => {
    try {
      setLoading(true);
      setShowSkeleton(true);
      setShowContent(false);
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
      trackWorkflowStarted(true, undefined, false);
      setView('pair', {
        disableAnimation: true,
        resumeSessionId: session.id,
      });
    } catch (error) {
      console.error('Failed to load workflow:', error);
      const errorMsg = error instanceof Error ? error.message : 'Failed to load workflow';
      trackWorkflowStarted(false, getErrorType(error), false);
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
      trackWorkflowStarted(true, undefined, true);
    } catch (error) {
      console.error('Failed to open workflow in new window:', error);
      trackWorkflowStarted(false, getErrorType(error), true);
    }
  };

  const handleDeleteWorkflow = async (workflowManifest: WorkflowManifest) => {
    const result = await window.electron.showMessageBox({
      type: 'warning',
      buttons: ['Cancel', 'Delete'],
      defaultId: 0,
      title: 'Delete Workflow',
      message: `Are you sure you want to delete "${workflowManifest.workflow.title}"?`,
      detail: 'Workflow file will be deleted.',
    });

    if (result.response !== 1) {
      return;
    }

    try {
      await deleteWorkflow({ body: { id: workflowManifest.id } });
      trackWorkflowDeleted(true);
      await loadSavedWorkflows();
      toastSuccess({
        title: workflowManifest.workflow.title,
        msg: 'Workflow deleted successfully',
      });
    } catch (err) {
      console.error('Failed to delete workflow:', err);
      const errorMsg = err instanceof Error ? err.message : 'Failed to delete workflow';
      trackWorkflowDeleted(false, getErrorType(err));
      setError(errorMsg);
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
      trackWorkflowDeeplinkCopied(true);
      toastSuccess({
        title: 'Deeplink copied',
        msg: 'Workflow deeplink has been copied to clipboard',
      });
    } catch (error) {
      console.error('Failed to copy deeplink:', error);
      trackWorkflowDeeplinkCopied(false, getErrorType(error));
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
      trackWorkflowYamlCopied(true);
      toastSuccess({
        title: 'YAML copied',
        msg: 'Workflow YAML has been copied to clipboard',
      });
    } catch (error) {
      console.error('Failed to copy YAML:', error);
      trackWorkflowYamlCopied(false, getErrorType(error));
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
        title: 'Export Workflow',
        defaultPath: filename,
        filters: [
          { name: 'YAML Files', extensions: ['yaml', 'yml'] },
          { name: 'All Files', extensions: ['*'] },
        ],
      });

      if (!result.canceled && result.filePath) {
        await window.electron.writeFile(result.filePath, response.data.yaml);
        trackWorkflowExportedToFile(true);
        toastSuccess({
          title: 'Workflow exported',
          msg: `Workflow saved to ${result.filePath}`,
        });
      }
    } catch (error) {
      console.error('Failed to export workflow:', error);
      trackWorkflowExportedToFile(false, getErrorType(error));
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
    if (!scheduleWorkflowManifest) return;

    const action = scheduleWorkflowManifest.schedule_cron ? 'edit' : 'add';

    try {
      await scheduleWorkflow({
        body: {
          id: scheduleWorkflowManifest.id,
          cron_schedule: scheduleCron,
        },
      });

      trackWorkflowScheduled(true, action);
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
      trackWorkflowScheduled(false, action, getErrorType(error));
      setError(errorMsg);
    }
  };

  const handleRemoveSchedule = async () => {
    if (!scheduleWorkflowManifest) return;

    try {
      await scheduleWorkflow({
        body: {
          id: scheduleWorkflowManifest.id,
          cron_schedule: null,
        },
      });

      trackWorkflowScheduled(true, 'remove');
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
      trackWorkflowScheduled(false, 'remove', getErrorType(error));
      setError(errorMsg);
    }
  };

  const handleOpenSlashCommandDialog = (workflowManifest: WorkflowManifest) => {
    setSlashCommandWorkflowManifest(workflowManifest);
    setSlashCommand(workflowManifest.slash_command || '');
    setShowSlashCommandDialog(true);
  };

  const handleSaveSlashCommand = async () => {
    if (!slashCommandWorkflowManifest) return;

    const action = slashCommand
      ? slashCommandWorkflowManifest.slash_command
        ? 'edit'
        : 'add'
      : 'remove';

    try {
      await setWorkflowSlashCommand({
        body: {
          id: slashCommandWorkflowManifest.id,
          slash_command: slashCommand || null,
        },
      });

      trackWorkflowSlashCommandSet(true, action);
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
      trackWorkflowSlashCommandSet(false, action, getErrorType(error));
      setError(errorMsg);
    }
  };

  const handleRemoveSlashCommand = async () => {
    if (!slashCommandWorkflowManifest) return;

    try {
      await setWorkflowSlashCommand({
        body: {
          id: slashCommandWorkflowManifest.id,
          slash_command: null,
        },
      });

      trackWorkflowSlashCommandSet(true, 'remove');
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
      trackWorkflowSlashCommandSet(false, 'remove', getErrorType(error));
      setError(errorMsg);
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
    workflowManifestResponse: { workflow, last_modified: lastModified, schedule_cron, slash_command },
  }: {
    workflowManifestResponse: WorkflowManifest;
  }) => (
    <div className="py-3 px-4 mb-1.5 bg-background-default rounded-xl border border-border-subtle hover:bg-background-medium transition-colors duration-150">
      <div className="flex justify-between items-start gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 mb-1">
            <h3 className="text-base truncate max-w-[50vw]">{workflow.title}</h3>
          </div>
          <p className="text-text-muted text-sm mb-2 line-clamp-2">{workflow.description}</p>
          <div className="flex flex-col gap-1 text-xs text-text-muted">
            <div className="flex items-center">
              <Calendar className="w-3 h-3 mr-1" />
              {convertToLocaleDateString(lastModified)}
            </div>
            {(schedule_cron || slash_command) && (
              <div className="flex items-center gap-3">
                {schedule_cron && (
                  <div className="flex items-center text-blue-600 dark:text-blue-400">
                    <Clock className="w-3 h-3 mr-1" />
                    Runs {getReadableCron(schedule_cron)}
                  </div>
                )}
                {slash_command && (
                  <div className="flex items-center text-purple-600 dark:text-purple-400">
                    /{slash_command}
                  </div>
                )}
              </div>
            )}
          </div>
        </div>

        <Button
          onClick={(e) => {
            e.stopPropagation();
            handleOpenSlashCommandDialog(workflowManifestResponse);
          }}
          variant={slash_command ? 'default' : 'outline'}
          size="sm"
          className="h-8 w-8 p-0"
          title={slash_command ? 'Edit slash command' : 'Add slash command'}
        >
          <Terminal className="w-4 h-4" />
        </Button>

        <div className="flex items-center gap-2 shrink-0">
          <Button
            onClick={(e) => {
              e.stopPropagation();
              handleStartWorkflowChat(workflow, workflowManifestResponse.id);
            }}
            size="sm"
            className="h-8 w-8 p-0"
            title="Use workflow"
          >
            <Play className="w-4 h-4" />
          </Button>
          <Button
            onClick={(e) => {
              e.stopPropagation();
              handleStartWorkflowChatInNewWindow(workflowManifestResponse.id);
            }}
            variant="outline"
            size="sm"
            className="h-8 w-8 p-0"
            title="Open in new window"
          >
            <ExternalLink className="w-4 h-4" />
          </Button>
          <Button
            onClick={(e) => {
              e.stopPropagation();
              handleEditWorkflow(workflowManifestResponse);
            }}
            variant="outline"
            size="sm"
            className="h-8 w-8 p-0"
            title="Edit workflow"
          >
            <Edit className="w-4 h-4" />
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                onClick={(e) => e.stopPropagation()}
                variant="outline"
                size="sm"
                className="h-8 w-8 p-0"
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
            variant={schedule_cron ? 'default' : 'outline'}
            size="sm"
            className="h-8 w-8 p-0"
            title={schedule_cron ? 'Edit schedule' : 'Add schedule'}
          >
            <Clock className="w-4 h-4" />
          </Button>
          <Button
            onClick={(e) => {
              e.stopPropagation();
              handleDeleteWorkflow(workflowManifestResponse);
            }}
            variant="ghost"
            size="sm"
            className="h-8 w-8 p-0 text-red-500 hover:text-red-600 hover:bg-red-50 dark:hover:bg-red-900/20"
            title="Delete workflow"
          >
            <Trash2 className="w-4 h-4" />
          </Button>
        </div>
      </div>
    </div>
  );

  const WorkflowSkeleton = () => (
    <div className="py-3 px-4 mb-1.5 bg-background-default rounded-xl border border-border-subtle">
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
    if (loading || showSkeleton) {
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
          <AlertCircle className="h-12 w-12 text-red-500 mb-4" />
          <p className="text-lg mb-2">Error Loading Workflows</p>
          <p className="text-sm text-center mb-4">{error}</p>
          <Button onClick={loadSavedWorkflows} variant="default">
            Try Again
          </Button>
        </div>
      );
    }

    if (savedWorkflows.length === 0) {
      return (
        <div className="flex flex-col justify-center pt-2 h-full">
          <p className="text-lg">No saved workflows</p>
          <p className="text-sm text-text-muted">Workflow saved from chats will show up here.</p>
        </div>
      );
    }

    if (filteredWorkflows.length === 0 && searchTerm) {
      return (
        <div className="flex flex-col items-center justify-center h-full text-text-muted mt-4">
          <FileText className="h-12 w-12 mb-4" />
          <p className="text-lg mb-2">No matching workflows found</p>
          <p className="text-sm">Try adjusting your search terms</p>
        </div>
      );
    }

    return (
      <div className="space-y-2">
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
          <div className="px-8 pt-12 pb-6 flex-shrink-0 border-b border-border-subtle">
            <h1 className="text-2xl font-semibold tracking-tight mb-1 page-transition">Workflows</h1>
            <p className="text-sm text-text-muted mb-0">
              View and manage your saved workflows to quickly start new sessions with predefined
              configurations. {getSearchShortcutText()} to search.
            </p>
            <div className="flex gap-3 mt-5">
              <Button
                onClick={() => setShowCreateDialog(true)}
                variant="default"
                className="flex items-center gap-2"
              >
                <FileText className="w-4 h-4" />
                Create Workflow
              </Button>
              <ImportWorkflowButton onClick={() => setShowImportDialog(true)} />
            </div>
          </div>

          <div className="flex-1 min-h-0 relative px-8 pt-6">
            <ScrollArea className="h-full">
              <SearchView onSearch={(term) => setSearchTerm(term)} placeholder="Search workflows...">
                <div
                  className={`h-full relative transition-all duration-300 ${
                    showContent ? 'opacity-100 animate-in fade-in ' : 'opacity-0'
                  }`}
                >
                  {renderContent()}
                </div>
              </SearchView>
            </ScrollArea>
          </div>
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
        <Dialog open={showScheduleDialog} onOpenChange={setShowScheduleDialog}>
          <DialogContent className="max-w-md">
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
                  <Button variant="outline" onClick={handleRemoveSchedule}>
                    Remove Schedule
                  </Button>
                )}
                <Button variant="outline" onClick={() => setShowScheduleDialog(false)}>
                  Cancel
                </Button>
                <Button onClick={handleSaveSchedule} disabled={!scheduleValid}>
                  Save
                </Button>
              </div>
            </div>
          </DialogContent>
        </Dialog>
      )}

      {showSlashCommandDialog && slashCommandWorkflowManifest && (
        <Dialog open={showSlashCommandDialog} onOpenChange={setShowSlashCommandDialog}>
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>Slash Command</DialogTitle>
            </DialogHeader>
            <div className="space-y-4">
              <div>
                <p className="text-sm text-muted-foreground mb-3">
                  Set a slash command to quickly run this workflow from any chat
                </p>
                <div className="flex gap-2 items-center">
                  <span className="text-muted-foreground">/</span>
                  <input
                    type="text"
                    value={slashCommand}
                    onChange={(e) => setSlashCommand(e.target.value)}
                    placeholder="command-name"
                    className="flex-1 px-3 py-2 border rounded text-sm"
                  />
                </div>
                {slashCommand && (
                  <p className="text-xs text-muted-foreground mt-2">
                    Use /{slashCommand} in any chat to run this workflow
                  </p>
                )}
              </div>

              <div className="flex gap-2 justify-end">
                {slashCommandWorkflowManifest.slash_command && (
                  <Button variant="outline" onClick={handleRemoveSlashCommand}>
                    Remove
                  </Button>
                )}
                <Button variant="outline" onClick={() => setShowSlashCommandDialog(false)}>
                  Cancel
                </Button>
                <Button onClick={handleSaveSlashCommand}>Save</Button>
              </div>
            </div>
          </DialogContent>
        </Dialog>
      )}
    </>
  );
}
