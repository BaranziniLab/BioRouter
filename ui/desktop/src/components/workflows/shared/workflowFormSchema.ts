import { z } from 'zod';

// Zod schema for Parameter - matching API WorkflowParameter type
const parameterSchema = z.object({
  key: z.string().min(1, 'Parameter key is required'),
  input_type: z.enum(['string', 'number', 'boolean', 'date', 'file', 'select']),
  requirement: z.enum(['required', 'optional', 'user_prompt']),
  description: z.string().min(1, 'Parameter description is required'),
  default: z.string().nullable().optional(),
  options: z.array(z.string()).nullable().optional(),
});

// Export the parameter type for use in components
export type WorkflowParameter = z.infer<typeof parameterSchema>;

// Main workflow form schema
export const workflowFormSchema = z.object({
  title: z
    .string()
    .min(1, 'Title is required')
    .min(3, 'Title must be at least 3 characters')
    .max(100, 'Title must be 100 characters or less'),

  description: z
    .string()
    .min(1, 'Description is required')
    .min(10, 'Description must be at least 10 characters')
    .max(500, 'Description must be 500 characters or less'),

  instructions: z
    .string()
    .min(1, 'Instructions are required')
    .min(20, 'Instructions must be at least 20 characters'),

  prompt: z.string().optional(),

  activities: z.array(z.string()).default([]),

  parameters: z.array(parameterSchema).default([]),

  jsonSchema: z.string().optional(),

  settings: z
    .object({
      biorouter_provider: z.string().optional(),
      biorouter_model: z.string().optional(),
      temperature: z.number().min(0).max(2).optional(),
    })
    .optional(),
});

export type WorkflowFormData = z.infer<typeof workflowFormSchema>;

// Type for the form API - using any to avoid complex generic constraints
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type WorkflowFormApi = any;
