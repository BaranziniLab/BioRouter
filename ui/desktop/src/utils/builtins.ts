// Mirrors crates/biorouter/src/knowledge/soul.rs — the built-in Soul assets
// (knowledge base, Meditation workflow, Daily Meditation schedule) that the
// backend installs on startup and re-creates if deleted.
export const SOUL_KB_ID = 'soul';
export const MEDITATION_WORKFLOW_FILE = 'meditation.yaml';
export const MEDITATION_SCHEDULE_ID = 'daily-meditation';
export const MEDITATION_SCHEDULE_DISPLAY_NAME = 'Daily Meditation';

export const BUILTIN_RECREATED_TITLE = 'Ships with Biorouter. Recreated automatically if deleted.';

export function isBuiltinKnowledgeBase(kbId: string): boolean {
  return kbId === SOUL_KB_ID;
}

// Workflow manifest ids are path hashes, so built-in workflows are identified
// by their well-known file name in the workflow library.
export function isBuiltinWorkflow(filePath: string): boolean {
  return filePath.replace(/\\/g, '/').split('/').pop() === MEDITATION_WORKFLOW_FILE;
}

export function isBuiltinSchedule(scheduleId: string): boolean {
  return scheduleId === MEDITATION_SCHEDULE_ID;
}

// Schedule ids double as on-disk filenames in the backend scheduler, so the
// built-in job keeps a slug id and gets a friendly name in the UI.
export function scheduleDisplayName(scheduleId: string): string {
  return scheduleId === MEDITATION_SCHEDULE_ID ? MEDITATION_SCHEDULE_DISPLAY_NAME : scheduleId;
}
