import { Workflow } from '../workflow';
import { Message } from '../api';

export interface ChatType {
  sessionId: string;
  name: string;
  messages: Message[];
  workflow?: Workflow | null; // Add workflow configuration to chat state
  resolvedWorkflow?: Workflow | null; // Add resolved workflow with parameter values rendered to chat state
  workflowParameterValues?: Record<string, string> | null; // Add workflow parameters to chat state
}
