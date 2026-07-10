import {
  encodeWorkflow as apiEncodeWorkflow,
  decodeWorkflow as apiDecodeWorkflow,
  scanWorkflow as apiScanWorkflow,
} from '../api';
import type { WorkflowParameter } from '../api';

// Re-export OpenAPI types with frontend-specific additions
export type Parameter = WorkflowParameter;
export interface WorkflowKnowledgeBases {
  default?: string | null;
  visible?: string[];
}
export type Workflow = import('../api').Workflow & {
  knowledge_bases?: WorkflowKnowledgeBases | null;
  skills?: string[] | null;
  // TODO: Separate these from the raw workflow type
  // Properties added for scheduled execution
  scheduledJobId?: string;
  isScheduledExecution?: boolean;
};

export async function encodeWorkflow(workflow: Workflow): Promise<string> {
  try {
    const response = await apiEncodeWorkflow({
      body: { workflow },
    });

    if (!response.data) {
      throw new Error('No data returned from API');
    }

    return response.data.deeplink;
  } catch (error) {
    console.error('Failed to encode workflow:', error);
    throw error;
  }
}

export async function decodeWorkflow(deeplink: string): Promise<Workflow> {
  console.log('Decoding workflow from deeplink:', deeplink);

  try {
    const response = await apiDecodeWorkflow({
      body: { deeplink },
    });

    if (!response.data) {
      throw new Error('No data returned from API');
    }

    if (!response.data.workflow) {
      console.error('Decoded workflow is null:', response.data);
      throw new Error('Decoded workflow is null');
    }

    return response.data.workflow as Workflow;
  } catch (error) {
    console.error('Failed to decode deeplink:', error);
    throw error;
  }
}

export async function scanWorkflow(
  workflow: Workflow
): Promise<{ has_security_warnings: boolean }> {
  try {
    const response = await apiScanWorkflow({
      body: { workflow },
    });

    if (!response.data) {
      throw new Error('No data returned from API');
    }

    return response.data;
  } catch (error) {
    console.error('Failed to scan workflow:', error);
    throw error;
  }
}

/**
 * A deep link carries the whole workflow as base64 in a URL that gets pasted
 * into chats and browsers. Literal `envs` values on an extension are secrets
 * (API keys, passcodes), so they must never travel with it — only `env_keys`,
 * which the recipient resolves from their own keyring.
 */
export function stripExtensionSecrets(workflow: Workflow): Workflow {
  if (!workflow.extensions?.length) {
    return workflow;
  }
  return {
    ...workflow,
    extensions: workflow.extensions.map((extension) =>
      'envs' in extension && extension.envs ? { ...extension, envs: undefined } : extension
    ) as Workflow['extensions'],
  };
}

export async function generateDeepLink(workflow: Workflow): Promise<string> {
  const encoded = await encodeWorkflow(stripExtensionSecrets(workflow));
  return `biorouter://workflow?config=${encoded}`;
}
