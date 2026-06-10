import React, { useState, useEffect } from 'react';
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
import { TrashIcon } from '../icons/TrashIcon';
import { Plus, RefreshCw, Pause, Play, Edit, Square, Eye, CircleDotDashed } from '../icons/app-icons';
import { NewSchedulePayload, ScheduleModal } from './ScheduleModal';
import ScheduleDetailView from './ScheduleDetailView';
import { toastError, toastSuccess } from '../../toasts';
import cronstrue from 'cronstrue';
import { formatToLocalDateWithTimezone } from '../../utils/date';
import { MainPanelLayout } from '../Layout/MainPanelLayout';
import { ViewOptions } from '../../utils/navigationUtils';
import { trackScheduleCreated, trackScheduleDeleted, getErrorType } from '../../utils/analytics';

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
    <div
      className="py-4 px-2 border-b border-border-subtle last:border-b-0 cursor-pointer hover:bg-background-medium/40 transition-colors duration-150"
      onClick={() => onNavigateToDetail(job.id)}
    >
      <div className="flex justify-between items-start gap-4">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 mb-1">
            <h3 className="text-base truncate max-w-[50vw]" title={job.id}>
              {job.id}
            </h3>
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
          </div>
          <p className="text-text-muted text-sm mb-2 line-clamp-2" title={readableCron}>
            {readableCron}
          </p>
          <div className="flex items-center text-xs text-text-muted">
            <span>Last run: {formattedLastRun}</span>
          </div>
        </div>

        <div className="flex items-center gap-2 shrink-0">
          {!job.currently_running && (
            <>
              <Button
                onClick={(e) => {
                  e.stopPropagation();
                  onEdit(job);
                }}
                disabled={actionInProgress}
                variant="outline"
                size="sm"
                className="h-8"
              >
                <Edit className="w-4 h-4 mr-1" />
                Edit
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
                variant="outline"
                size="sm"
                className="h-8"
              >
                {job.paused ? (
                  <>
                    <Play className="w-4 h-4 mr-1" />
                    Resume
                  </>
                ) : (
                  <>
                    <Pause className="w-4 h-4 mr-1" />
                    Pause
                  </>
                )}
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
                variant="outline"
                size="sm"
                className="h-8"
              >
                <Eye className="w-4 h-4 mr-1" />
                Inspect
              </Button>
              <Button
                onClick={(e) => {
                  e.stopPropagation();
                  onKill(job.id);
                }}
                disabled={actionInProgress}
                variant="outline"
                size="sm"
                className="h-8"
              >
                <Square className="w-4 h-4 mr-1" />
                Kill
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
            className="h-8 text-text-danger hover:bg-background-danger/10"
          >
            <TrashIcon className="w-4 h-4" />
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
  const [viewingScheduleId, setViewingScheduleId] = useState<string | null>(null);

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
          title: 'Schedule Updated',
          msg: `Successfully updated schedule "${editingSchedule.id}"`,
        });
      } else {
        const newPayload = payload as NewSchedulePayload;
        await createSchedule(newPayload);
        trackScheduleCreated('file', true);
      }
      await fetchSchedules();
      setIsModalOpen(false);
      setEditingSchedule(null);
    } catch (error) {
      console.error('Failed to save schedule:', error);
      const errorMsg = error instanceof Error ? error.message : 'Unknown error saving schedule.';
      setSubmitApiError(errorMsg);

      if (!editingSchedule) {
        trackScheduleCreated('file', false, getErrorType(error));
      }
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleDeleteSchedule = async (id: string) => {
    if (!window.confirm(`Are you sure you want to delete schedule "${id}"?`)) return;

    setActionsInProgress((prev) => new Set(prev).add(id));
    if (viewingScheduleId === id) setViewingScheduleId(null);
    setApiError(null);

    try {
      await deleteSchedule(id);
      trackScheduleDeleted(true);
      await fetchSchedules();
    } catch (error) {
      console.error(`Failed to delete schedule "${id}":`, error);
      const errorMsg = error instanceof Error ? error.message : `Unknown error deleting "${id}".`;
      setApiError(errorMsg);
      trackScheduleDeleted(false, getErrorType(error));
    } finally {
      setActionsInProgress((prev) => {
        const newSet = new Set(prev);
        newSet.delete(id);
        return newSet;
      });
    }
  };

  const handlePauseSchedule = async (id: string) => {
    setActionsInProgress((prev) => new Set(prev).add(id));
    setApiError(null);

    try {
      await pauseSchedule(id);
      toastSuccess({
        title: 'Schedule Paused',
        msg: `Successfully paused schedule "${id}"`,
      });
      await fetchSchedules();
    } catch (error) {
      console.error(`Failed to pause schedule "${id}":`, error);
      const errorMsg = error instanceof Error ? error.message : `Unknown error pausing "${id}".`;
      setApiError(errorMsg);
      toastError({
        title: 'Pause Schedule Error',
        msg: errorMsg,
      });
    } finally {
      setActionsInProgress((prev) => {
        const newSet = new Set(prev);
        newSet.delete(id);
        return newSet;
      });
    }
  };

  const handleUnpauseSchedule = async (id: string) => {
    setActionsInProgress((prev) => new Set(prev).add(id));
    setApiError(null);

    try {
      await unpauseSchedule(id);
      toastSuccess({
        title: 'Schedule Unpaused',
        msg: `Successfully unpaused schedule "${id}"`,
      });
      await fetchSchedules();
    } catch (error) {
      console.error(`Failed to unpause schedule "${id}":`, error);
      const errorMsg = error instanceof Error ? error.message : `Unknown error unpausing "${id}".`;
      setApiError(errorMsg);
      toastError({
        title: 'Unpause Schedule Error',
        msg: errorMsg,
      });
    } finally {
      setActionsInProgress((prev) => {
        const newSet = new Set(prev);
        newSet.delete(id);
        return newSet;
      });
    }
  };

  const handleKillRunningJob = async (id: string) => {
    setActionsInProgress((prev) => new Set(prev).add(id));
    setApiError(null);

    try {
      const result = await killRunningJob(id);
      toastSuccess({
        title: 'Job Killed',
        msg: result.message,
      });
      await fetchSchedules();
    } catch (error) {
      console.error(`Failed to kill running job "${id}":`, error);
      const errorMsg =
        error instanceof Error ? error.message : `Unknown error killing job "${id}".`;
      setApiError(errorMsg);
      toastError({
        title: 'Kill Job Error',
        msg: errorMsg,
      });
    } finally {
      setActionsInProgress((prev) => {
        const newSet = new Set(prev);
        newSet.delete(id);
        return newSet;
      });
    }
  };

  const handleInspectRunningJob = async (id: string) => {
    setActionsInProgress((prev) => new Set(prev).add(id));
    setApiError(null);

    try {
      const result = await inspectRunningJob(id);
      if (result.sessionId) {
        const duration = result.runningDurationSeconds
          ? `${Math.floor(result.runningDurationSeconds / 60)}m ${result.runningDurationSeconds % 60}s`
          : 'Unknown';
        toastSuccess({
          title: 'Job Inspection',
          msg: `Session: ${result.sessionId}\nRunning for: ${duration}`,
        });
      } else {
        toastSuccess({
          title: 'Job Inspection',
          msg: 'No detailed information available for this job',
        });
      }
    } catch (error) {
      console.error(`Failed to inspect running job "${id}":`, error);
      const errorMsg =
        error instanceof Error ? error.message : `Unknown error inspecting job "${id}".`;
      setApiError(errorMsg);
      toastError({
        title: 'Inspect Job Error',
        msg: errorMsg,
      });
    } finally {
      setActionsInProgress((prev) => {
        const newSet = new Set(prev);
        newSet.delete(id);
        return newSet;
      });
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
          <div className="px-8 pt-12 pb-6 flex-shrink-0 border-b border-border-subtle">
            <h1 className="text-2xl font-semibold tracking-tight mb-1 page-transition">Scheduler</h1>
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
          </div>

          <div className="flex-1 min-h-0 relative px-8 pt-6">
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
                  <div className="flex flex-col pt-4 pb-12">
                    <CircleDotDashed className="h-5 w-5 text-text-muted mb-3.5" />
                    <p className="text-base text-text-muted mb-2">No schedules yet</p>
                  </div>
                )}

                {!isLoading && schedules.length > 0 && (
                  <div className="space-y-2 pb-8">
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
                        onDelete={handleDeleteSchedule}
                        actionInProgress={actionsInProgress.has(job.id) || isSubmitting}
                      />
                    ))}
                  </div>
                )}
              </div>
            </ScrollArea>
          </div>
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
    </>
  );
};

export default SchedulesView;
