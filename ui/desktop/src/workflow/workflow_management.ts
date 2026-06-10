import { saveWorkflow as saveWorkflowApi, listWorkflows, WorkflowManifest } from '../api';
import type { Workflow } from './index';

export const saveWorkflow = async (workflow: Workflow, workflowId?: string | null): Promise<string> => {
  try {
    let response = await saveWorkflowApi({
      body: {
        workflow,
        id: workflowId,
      },
      throwOnError: true,
    });
    return response.data.id;
  } catch (error) {
    let error_message = 'unknown error';
    if (typeof error === 'object' && error !== null && 'message' in error) {
      error_message = error.message as string;
    }
    throw new Error(error_message);
  }
};

export const listSavedWorkflows = async (): Promise<WorkflowManifest[]> => {
  try {
    const listWorkflowResponse = await listWorkflows();
    return listWorkflowResponse?.data?.manifests ?? [];
  } catch (error) {
    console.warn('Failed to list saved workflows:', error);
    return [];
  }
};

const parseLastModified = (val: string | Date): Date => {
  return val instanceof Date ? val : new Date(val);
};

export const convertToLocaleDateString = (lastModified: string): string => {
  if (lastModified) {
    return parseLastModified(lastModified).toLocaleDateString();
  }
  return '';
};

export const getStorageDirectory = (isGlobal: boolean): string => {
  if (isGlobal) {
    return '~/.config/biorouter/workflows';
  } else {
    // For directory workflows, build absolute path using working directory
    const workingDir = window.appConfig.get('BIOROUTER_WORKING_DIR') as string;
    return `${workingDir}/.biorouter/workflows`;
  }
};
