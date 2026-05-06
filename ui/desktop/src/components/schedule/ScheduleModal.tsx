import React, { useState, useEffect, FormEvent } from 'react';
import { Button } from '../ui/button';
import { Input } from '../ui/input';
import { ScheduledJob } from '../../schedule';
import { CronPicker } from './CronPicker';
import { getStorageDirectory } from '../../workflow/workflow_management';
import { Folder } from '../icons/app-icons';
import ClockIcon from '../../assets/clock-icon.svg';

export interface NewSchedulePayload {
  id: string;
  workflow_source: string;
  cron: string;
  execution_mode?: string;
}

interface ScheduleModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSubmit: (payload: NewSchedulePayload | string) => Promise<void>;
  schedule: ScheduledJob | null;
  isLoadingExternally: boolean;
  apiErrorExternally: string | null;
  initialDeepLink?: string | null;
}


const modalLabelClassName = 'block text-sm font-medium text-text-default mb-1';

export const ScheduleModal: React.FC<ScheduleModalProps> = ({
  isOpen,
  onClose,
  onSubmit,
  schedule,
  isLoadingExternally,
  apiErrorExternally,
}) => {
  const isEditMode = !!schedule;

  const [scheduleId, setScheduleId] = useState<string>('');
  const [workflowSourcePath, setWorkflowSourcePath] = useState<string>('');
  const [cronExpression, setCronExpression] = useState<string>('0 0 14 * * *');
  const [internalValidationError, setInternalValidationError] = useState<string | null>(null);
  const [isValid, setIsValid] = useState(true);

  useEffect(() => {
    if (isOpen) {
      if (schedule) {
        setScheduleId(schedule.id);
        setCronExpression(schedule.cron);
      } else {
        setScheduleId('');
        setWorkflowSourcePath('');
        setCronExpression('0 0 14 * * *');
        setInternalValidationError(null);
      }
    }
  }, [isOpen, schedule]);

  const handleBrowseFile = async () => {
    const defaultPath = getStorageDirectory(true);
    const filePath = await window.electron.selectFileOrDirectory(defaultPath);
    if (filePath) {
      if (filePath.endsWith('.yaml') || filePath.endsWith('.yml')) {
        setWorkflowSourcePath(filePath);
        setInternalValidationError(null);
      } else {
        setInternalValidationError('Invalid file type: Please select a YAML file (.yaml or .yml)');
      }
    }
  };

  const handleLocalSubmit = async (event: FormEvent) => {
    event.preventDefault();
    setInternalValidationError(null);

    if (isEditMode) {
      await onSubmit(cronExpression);
      return;
    }

    if (!scheduleId.trim()) {
      setInternalValidationError('Schedule ID is required.');
      return;
    }

    if (!workflowSourcePath) {
      setInternalValidationError('Workflow source file is required.');
      return;
    }

    const finalWorkflowSource = workflowSourcePath;

    const newSchedulePayload: NewSchedulePayload = {
      id: scheduleId.trim(),
      workflow_source: finalWorkflowSource,
      cron: cronExpression,
    };

    await onSubmit(newSchedulePayload);
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 bg-black/50 z-40 flex items-center justify-center p-4">
      <div className="w-full max-w-md bg-background-default border border-border-subtle rounded-xl z-50 flex flex-col max-h-[90vh] overflow-hidden" style={{ boxShadow: 'var(--shadow-popover)' }}>
        <div className="px-6 pt-5 pb-4 flex-shrink-0 border-b border-border-subtle">
          <div className="flex items-center gap-3">
            <img src={ClockIcon} alt="Clock" className="w-7 h-7" />
            <div className="flex-1">
              <h2 className="text-base font-semibold text-text-default">
                {isEditMode ? 'Edit Schedule' : 'Create New Schedule'}
              </h2>
              {isEditMode && <p className="text-xs text-text-muted mt-0.5">{schedule.id}</p>}
            </div>
          </div>
        </div>

        <form
          id="schedule-form"
          onSubmit={handleLocalSubmit}
          className="px-6 py-5 space-y-5 flex-grow overflow-y-auto"
        >
          {apiErrorExternally && (
            <p className="text-red-600 dark:text-red-400 text-sm mb-3 p-2 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800/50 rounded-md">
              {apiErrorExternally}
            </p>
          )}
          {internalValidationError && (
            <p className="text-red-600 dark:text-red-400 text-sm mb-3 p-2 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800/50 rounded-md">
              {internalValidationError}
            </p>
          )}

          {!isEditMode && (
            <>
              <div>
                <label htmlFor="scheduleId-modal" className={modalLabelClassName}>
                  Name
                </label>
                <Input
                  type="text"
                  id="scheduleId-modal"
                  value={scheduleId}
                  onChange={(e) => setScheduleId(e.target.value)}
                  placeholder="e.g., daily-summary-job"
                  required
                />
              </div>

              <div>
                <label className={modalLabelClassName}>Workflow File</label>
                <div className="flex items-center rounded-lg border border-border-subtle bg-background-default focus-within:border-border-strong transition-colors duration-150">
                  <input
                    type="text"
                    value={workflowSourcePath}
                    onChange={(e) => {
                      setWorkflowSourcePath(e.target.value);
                      setInternalValidationError(null);
                    }}
                    placeholder="/path/to/workflow.yaml"
                    className="flex-1 px-3 py-2 text-sm bg-transparent text-text-default placeholder:text-text-muted focus:outline-none"
                  />
                  <button
                    type="button"
                    onClick={handleBrowseFile}
                    title="Browse for YAML file"
                    className="px-2.5 py-2 text-text-muted hover:text-text-default transition-colors duration-150"
                  >
                    <Folder className="w-4 h-4" />
                  </button>
                </div>
                <p className="mt-1 text-xs text-text-muted">Select a YAML workflow file (.yaml or .yml)</p>
              </div>
            </>
          )}

          <div>
            <label className={modalLabelClassName}>Schedule</label>
            <CronPicker schedule={schedule} onChange={setCronExpression} isValid={setIsValid} />
          </div>
        </form>

        <div className="flex gap-2 px-6 py-4 border-t border-border-subtle">
          <Button
            type="button"
            variant="ghost"
            onClick={onClose}
            disabled={isLoadingExternally}
            className="flex-1"
          >
            Cancel
          </Button>
          <Button
            type="submit"
            form="schedule-form"
            disabled={isLoadingExternally || !isValid}
            className="flex-1"
          >
            {isLoadingExternally
              ? isEditMode
                ? 'Updating...'
                : 'Creating...'
              : isEditMode
                ? 'Update Schedule'
                : 'Create Schedule'}
          </Button>
        </div>
      </div>
    </div>
  );
};
