import React, { useState, useEffect, useRef } from 'react';
import { useLocation } from 'react-router-dom';
import {
  listSchedules,
  createSchedule,
  deleteSchedule,
  pauseSchedule,
  unpauseSchedule,
  updateSchedule,
  killRunningJob,
  inspectRunningJob,
  ScheduledJob,
} from '../../schedule';
import { ScrollArea } from '../ui/scroll-area';
import { Button } from '../ui/button';
import {
  Plus,
  RefreshCw,
  Pause,
  Play,
  Edit,
  Square,
  Eye,
  CircleDotDashed,
  Trash2,
  AlertTriangle,
} from '../icons/app-icons';
import { NewSchedulePayload, ScheduleModal } from './ScheduleModal';
import ScheduleDetailView from './ScheduleDetailView';
import { toastError, toastSuccess } from '../../toasts';
import cronstrue from 'cronstrue';
import { formatToLocalDateWithTimezone } from '../../utils/date';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { ViewOptions } from '../../utils/navigationUtils';
import BuiltInBadge from '../ui/BuiltInBadge';
import {
  BUILTIN_RECREATED_TITLE,
  isBuiltinSchedule,
  scheduleDisplayName,
} from '../../utils/builtins';
import { ReadableContent } from '../Layout/ReadableContent';
import { ConfirmationModal } from '../ui/ConfirmationModal';
import { EmptyState } from '../ui/empty-state';

interface SchedulesViewProps {
  onClose?: () => void;
}

const ScheduleCard: React.FC<{
  job: ScheduledJob;
  onNavigateToDetail: (id: string) => void;
  onEdit: (job: ScheduledJob) => void;
  onPause: (id: string) => void;
  onUnpause: (id: string) => void;
  onKill: (id: string) => void;
  onInspect: (id: string) => void;
  onDelete: (id: string) => void;
  actionInProgress: boolean;
}> = ({
  job,
  onNavigateToDetail,
  onEdit,
  onPause,
  onUnpause,
  onKill,
  onInspect,
  onDelete,
  actionInProgress,
}) => {
  let readableCron: string;
  try {
    readableCron = cronstrue.toString(job.cron);
  } catch {
    readableCron = job.cron;
  }

  const formattedLastRun = formatToLocalDateWithTimezone(job.last_run);

  return (
    <div className="biorouter-list-row group py-3 px-3">
      <div className="flex justify-between items-start gap-3">
        <button
          type="button"
          className="min-w-0 flex-1 cursor-pointer rounded-sm text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-border-focus"
          onClick={() => onNavigateToDetail(job.id)}
          aria-label={`View schedule ${scheduleDisplayName(job.id)}`}
        >
          <div className="flex items-center gap-1.5">
            <h3 className="text-sm text-text-default truncate max-w-[50vw]" title={job.id}>
              {scheduleDisplayName(job.id)}
            </h3>
            {isBuiltinSchedule(job.id) && <BuiltInBadge title={BUILTIN_RECREATED_TITLE} />}
            {job.currently_running && (
              <span className="inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium bg-background-success/15 text-text-success">
                <span className="inline-block w-2 h-2 bg-background-success rounded-full mr-1 animate-pulse"></span>
                Running
              </span>
            )}
            {job.paused && (
              <span className="inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium bg-background-warning/15 text-text-warning">
                <Pause className="w-3 h-3 mr-1" />
                Paused
              </span>
            )}
            {/*
              Issue #56. A schedule whose last tick failed looks identical to a
              healthy one on this list — a fresh session is minted per run, so
              there is nothing else here to notice. Cleared by the next success.
            */}
            {job.last_error && !job.currently_running && (
              <span className="inline-flex items-center px-2 py-0.5 rounded-md text-xs font-medium bg-background-danger/15 text-text-danger">
                <AlertTriangle className="w-3 h-3 mr-1" />
                Failed
              </span>
            )}
          </div>
          <p className="text-xs text-text-muted mt-0.5 line-clamp-1" title={readableCron}>
            {readableCron}
          </p>
          <div className="flex items-center text-[11px] text-text-subtle mt-1">
            <span>Last run: {formattedLastRun}</span>
          </div>
          {job.last_error && (
            <p
              className="text-[11px] text-text-danger mt-1 line-clamp-2 break-words"
              title={job.last_error}
            >
              {job.last_error}
            </p>
          )}
        </button>

        <div className="flex items-center gap-1 shrink-0 opacity-100 transition-opacity sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100">
          {!job.currently_running && (
            <>
              <Button
                onClick={(e) => {
                  e.stopPropagation();
                  onEdit(job);
                }}
                disabled={actionInProgress}
                variant="ghost"
                shape="round"
                title="Edit schedule"
                aria-label={`Edit ${scheduleDisplayName(job.id)}`}
              >
                <Edit className="w-4 h-4" />
              </Button>
              <Button
                onClick={(e) => {
                  e.stopPropagation();
                  if (job.paused) {
                    onUnpause(job.id);
                  } else {
                    onPause(job.id);
                  }
                }}
                disabled={actionInProgress}
                variant="ghost"
                shape="round"
                title={job.paused ? 'Resume this schedule' : 'Pause this schedule'}
                aria-label={`${job.paused ? 'Resume' : 'Pause'} ${scheduleDisplayName(job.id)}`}
              >
                {job.paused ? <Play className="w-4 h-4" /> : <Pause className="w-4 h-4" />}
              </Button>
            </>
          )}
          {job.currently_running && (
            <>
              <Button
                onClick={(e) => {
                  e.stopPropagation();
                  onInspect(job.id);
                }}
                disabled={actionInProgress}
                variant="ghost"
                shape="round"
                title="Show the current run"
                aria-label={`Inspect ${scheduleDisplayName(job.id)}`}
              >
                <Eye className="w-4 h-4" />
              </Button>
              <Button
                onClick={(e) => {
                  e.stopPropagation();
                  onKill(job.id);
                }}
                disabled={actionInProgress}
                variant="ghost"
                shape="round"
                title="Stop the running job"
                aria-label={`Kill ${scheduleDisplayName(job.id)}`}
              >
                <Square className="w-4 h-4" />
              </Button>
            </>
          )}
          <Button
            onClick={(e) => {
              e.stopPropagation();
              onDelete(job.id);
            }}
            disabled={actionInProgress}
            variant="ghost"
            size="sm"
            className="text-text-danger"
            title="Delete schedule"
            aria-label={`Delete ${scheduleDisplayName(job.id)}`}
          >
            <Trash2 className="w-4 h-4" />
          </Button>
        </div>
      </div>
    </div>
  );
};

const SchedulesView: React.FC<SchedulesViewProps> = ({ onClose: _onClose }) => {
  const location = useLocation();
  const [schedules, setSchedules] = useState<ScheduledJob[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [apiError, setApiError] = useState<string | null>(null);
  const [submitApiError, setSubmitApiError] = useState<string | null>(null);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [editingSchedule, setEditingSchedule] = useState<ScheduledJob | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [actionsInProgress, setActionsInProgress] = useState<Set<string>>(new Set());
  const actionsInProgressRef = useRef<Set<string>>(new Set());
  const [viewingScheduleId, setViewingScheduleId] = useState<string | null>(null);
  const [scheduleToDeleteId, setScheduleToDeleteId] = useState<string | null>(null);

  const beginAction = (id: string) => {
    if (actionsInProgressRef.current.has(id)) return false;
    actionsInProgressRef.current.add(id);
    setActionsInProgress(new Set(actionsInProgressRef.current));
    return true;
  };

  const finishAction = (id: string) => {
    actionsInProgressRef.current.delete(id);
    setActionsInProgress(new Set(actionsInProgressRef.current));
  };

  const fetchSchedules = async () => {
    setIsLoading(true);
    setApiError(null);
    try {
      const fetchedSchedules = await listSchedules();
      setSchedules(fetchedSchedules);
    } catch (error) {
      console.error('Failed to fetch schedules:', error);
      setApiError(
        error instanceof Error
          ? error.message
          : 'An unknown error occurred while fetching schedules.'
      );
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    if (viewingScheduleId === null) {
      fetchSchedules();

      const locationState = location.state as ViewOptions | null;
      if (locationState?.pendingScheduleDeepLink) {
        setIsModalOpen(true);
        window.history.replaceState({}, document.title);
      }
    }
  }, [viewingScheduleId, location.state]);

  useEffect(() => {
    if (viewingScheduleId !== null || actionsInProgress.size > 0) return;

    const intervalId = setInterval(() => {
      if (viewingScheduleId === null && !isRefreshing && !isLoading && !isSubmitting) {
        fetchSchedules();
      }
    }, 15000);

    return () => clearInterval(intervalId);
  }, [viewingScheduleId, isRefreshing, isLoading, isSubmitting, actionsInProgress.size]);

  const handleRefresh = async () => {
    setIsRefreshing(true);
    try {
      await fetchSchedules();
    } finally {
      setIsRefreshing(false);
    }
  };

  const handleModalSubmit = async (payload: NewSchedulePayload | string) => {
    setIsSubmitting(true);
    setSubmitApiError(null);
    try {
      if (editingSchedule) {
        await updateSchedule(editingSchedule.id, payload as string);
        toastSuccess({
          title: 'Schedule updated',
          msg: `Updated schedule "${editingSchedule.id}"`,
        });
      } else {
        const newPayload = payload as NewSchedulePayload;
        await createSchedule(newPayload);
      }
      await fetchSchedules();
      setIsModalOpen(false);
      setEditingSchedule(null);
    } catch (error) {
      console.error('Failed to save schedule:', error);
      const errorMsg = error instanceof Error ? error.message : 'Unknown error saving schedule.';
      setSubmitApiError(errorMsg);
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDeleteSchedule = async (id: string) => {
    if (!beginAction(id)) return;
    if (viewingScheduleId === id) setViewingScheduleId(null);
    setApiError(null);

    try {
      await deleteSchedule(id);
      await fetchSchedules();
    } catch (error) {
      console.error(`Failed to delete schedule "${id}":`, error);
      const errorMsg = error instanceof Error ? error.message : `Unknown error deleting "${id}".`;
      setApiError(errorMsg);
    } finally {
      finishAction(id);
      setScheduleToDeleteId(null);
    }
  };

  const handlePauseSchedule = async (id: string) => {
    if (!beginAction(id)) return;
    setApiError(null);

    try {
      await pauseSchedule(id);
      toastSuccess({
        title: 'Schedule paused',
        msg: `Paused schedule "${id}"`,
      });
      await fetchSchedules();
    } catch (error) {
      console.error(`Failed to pause schedule "${id}":`, error);
      const errorMsg = error instanceof Error ? error.message : `Unknown error pausing "${id}".`;
      setApiError(errorMsg);
      toastError({
        title: 'Pause schedule error',
        msg: errorMsg,
      });
    } finally {
      finishAction(id);
    }
  };

  const handleUnpauseSchedule = async (id: string) => {
    if (!beginAction(id)) return;
    setApiError(null);

    try {
      await unpauseSchedule(id);
      toastSuccess({
        title: 'Schedule unpaused',
        msg: `Resumed schedule "${id}"`,
      });
      await fetchSchedules();
    } catch (error) {
      console.error(`Failed to unpause schedule "${id}":`, error);
      const errorMsg = error instanceof Error ? error.message : `Unknown error unpausing "${id}".`;
      setApiError(errorMsg);
      toastError({
        title: 'Unpause schedule error',
        msg: errorMsg,
      });
    } finally {
      finishAction(id);
    }
  };

  const handleKillRunningJob = async (id: string) => {
    if (!beginAction(id)) return;
    setApiError(null);

    try {
      const result = await killRunningJob(id);
      toastSuccess({
        title: 'Job stopped',
        msg: result.message,
      });
      await fetchSchedules();
    } catch (error) {
      console.error(`Failed to kill running job "${id}":`, error);
      const errorMsg =
        error instanceof Error ? error.message : `Unknown error killing job "${id}".`;
      setApiError(errorMsg);
      toastError({
        title: 'Could not stop the job',
        msg: errorMsg,
      });
    } finally {
      finishAction(id);
    }
  };

  const handleInspectRunningJob = async (id: string) => {
    if (!beginAction(id)) return;
    setApiError(null);

    try {
      const result = await inspectRunningJob(id);
      if (result.sessionId) {
        const duration = result.runningDurationSeconds
          ? `${Math.floor(result.runningDurationSeconds / 60)}m ${result.runningDurationSeconds % 60}s`
          : 'Unknown';
        toastSuccess({
          title: 'Job inspection',
          msg: `Session: ${result.sessionId}\nRunning for: ${duration}`,
        });
      } else {
        toastSuccess({
          title: 'Job inspection',
          msg: 'No detailed information available for this job',
        });
      }
    } catch (error) {
      console.error(`Failed to inspect running job "${id}":`, error);
      const errorMsg =
        error instanceof Error ? error.message : `Unknown error inspecting job "${id}".`;
      setApiError(errorMsg);
      toastError({
        title: 'Inspect job error',
        msg: errorMsg,
      });
    } finally {
      finishAction(id);
    }
  };

  const handleNavigateToDetail = (id: string) => {
    setViewingScheduleId(id);
  };

  if (viewingScheduleId) {
    return (
      <ScheduleDetailView
        scheduleId={viewingScheduleId}
        onNavigateBack={() => setViewingScheduleId(null)}
      />
    );
  }

  return (
    <>
      <MainPanelLayout>
        <div className="flex-1 flex flex-col min-h-0">
          {/* Flat page header */}
          <div className="flex-shrink-0 border-b border-border-subtle">
            <ReadableContent className="px-8 pt-12 pb-6">
              <h1 className="text-2xl font-semibold tracking-tight mb-1 page-transition">
                Scheduler
              </h1>
              <p className="text-sm text-text-muted mb-0">
                Create and manage scheduled tasks to run workflows automatically at specified times.
              </p>
              <div className="flex gap-3 mt-5">
                <Button
                  onClick={() => {
                    setSubmitApiError(null);
                    setIsModalOpen(true);
                  }}
                  variant="default"
                  className="flex items-center gap-2"
                >
                  <Plus className="h-4 w-4" />
                  Create Schedule
                </Button>
                <Button
                  onClick={handleRefresh}
                  disabled={isRefreshing || isLoading}
                  variant="outline"
                  className="flex items-center gap-2"
                >
                  <RefreshCw className={`h-4 w-4 ${isRefreshing ? 'animate-spin' : ''}`} />
                  {isRefreshing ? 'Refreshing...' : 'Refresh'}
                </Button>
              </div>
            </ReadableContent>
          </div>

          <ReadableContent className="flex-1 min-h-0 relative px-8 pt-6">
            <ScrollArea className="h-full">
              <div className="h-full relative">
                {apiError && (
                  <div className="mb-4 p-4 bg-background-danger/10 border border-border-danger/40 rounded-md">
                    <p className="text-text-danger text-sm">Error: {apiError}</p>
                  </div>
                )}

                {isLoading && schedules.length === 0 && (
                  <div className="flex justify-center items-center py-12">
                    <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-text-default"></div>
                  </div>
                )}

                {!isLoading && !apiError && schedules.length === 0 && (
                  <EmptyState
                    icon={CircleDotDashed}
                    title="No schedules yet"
                    description="Create a schedule to run a saved workflow automatically at the time you choose."
                    actions={
                      <Button
                        onClick={() => {
                          setSubmitApiError(null);
                          setIsModalOpen(true);
                        }}
                      >
                        <Plus className="h-4 w-4" />
                        Create schedule
                      </Button>
                    }
                  />
                )}

                {!isLoading && schedules.length > 0 && (
                  <div className="biorouter-list-shell pb-8">
                    {schedules.map((job) => (
                      <ScheduleCard
                        key={job.id}
                        job={job}
                        onNavigateToDetail={handleNavigateToDetail}
                        onEdit={(schedule) => {
                          setEditingSchedule(schedule);
                          setSubmitApiError(null);
                          setIsModalOpen(true);
                        }}
                        onPause={handlePauseSchedule}
                        onUnpause={handleUnpauseSchedule}
                        onKill={handleKillRunningJob}
                        onInspect={handleInspectRunningJob}
                        onDelete={setScheduleToDeleteId}
                        actionInProgress={actionsInProgress.has(job.id) || isSubmitting}
                      />
                    ))}
                  </div>
                )}
              </div>
            </ScrollArea>
          </ReadableContent>
        </div>
      </MainPanelLayout>

      <ScheduleModal
        isOpen={isModalOpen}
        onClose={() => {
          setIsModalOpen(false);
          setEditingSchedule(null);
          setSubmitApiError(null);
        }}
        onSubmit={handleModalSubmit}
        schedule={editingSchedule}
        isLoadingExternally={isSubmitting}
        apiErrorExternally={submitApiError}
      />
      <ConfirmationModal
        isOpen={scheduleToDeleteId !== null}
        title={`Delete "${scheduleToDeleteId ?? ''}"?`}
        message="This permanently removes the schedule and its run configuration. This action cannot be undone."
        confirmLabel="Delete"
        cancelLabel="Cancel"
        confirmVariant="destructive"
        isSubmitting={scheduleToDeleteId !== null && actionsInProgress.has(scheduleToDeleteId)}
        onConfirm={() => scheduleToDeleteId && void handleDeleteSchedule(scheduleToDeleteId)}
        onCancel={() => setScheduleToDeleteId(null)}
      />
    </>
  );
};

export default SchedulesView;
