import { z } from 'zod';

/**
 * Validation schema for workflow names
 */
export const workflowNameSchema = z.string().min(3, 'Workflow name must be at least 3 characters');

/**
 * Transform a string to a valid workflow name format:
 * - Convert to lowercase
 * - Replace spaces with dashes
 * - Remove invalid characters
 * - Trim whitespace and dashes
 */
export function transformToWorkflowName(input: string): string {
  return input
    .toLowerCase()
    .replace(/[^a-zA-Z0-9\s-]/g, '') // Remove invalid characters
    .replace(/\s+/g, '-') // Replace spaces with dashes
    .replace(/--+/g, '-') // Replace multiple dashes with single dash
    .replace(/^-+|-+$/g, '') // Remove leading/trailing dashes
    .trim();
}

/**
 * Generate a workflow name from a title
 */
export function generateWorkflowNameFromTitle(title: string): string {
  if (!title.trim()) {
    return '';
  }
  return transformToWorkflowName(title);
}

/**
 * Common placeholder text for workflow name inputs
 */
export const WORKFLOW_NAME_PLACEHOLDER = 'my-awesome-workflow';
