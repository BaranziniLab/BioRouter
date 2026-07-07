import { describe, expect, it } from 'vitest';
import {
  modelSupportedInputMimeTypes,
  modelSupportsVision,
  resolveKnownModelInfo,
} from './modelInterface';
import type { ProviderMetadata } from '../../../api';

const metadata: ProviderMetadata = {
  name: 'versa_azure',
  display_name: 'Versa API Azure',
  description: 'Test provider',
  default_model: 'gpt-5.5-2026-04-24',
  known_models: [
    { name: 'gpt-5.5', context_limit: 1_050_000, supports_vision: true },
    { name: 'gpt-5.3-codex', context_limit: 400_000 },
    { name: 'text-only', context_limit: 128_000 },
  ],
  model_doc_link: '',
  config_keys: [],
  allows_unlisted_models: true,
};

const claudeMetadata: ProviderMetadata = {
  name: 'claude_test',
  display_name: 'Claude Test',
  description: 'Test provider',
  default_model: 'claude-sonnet-4-6',
  known_models: [
    { name: 'claude-sonnet-4-6', context_limit: 1_000_000, supports_vision: true },
    {
      name: 'claude-opus-4-6',
      context_limit: 1_000_000,
      supports_vision: true,
      supported_input_mime_types: ['image/png', 'image/jpeg', 'image/jpg', 'image/gif', 'image/webp'],
    },
    { name: 'claude-sonnet-4-20250514', context_limit: 200_000, supports_vision: true },
  ],
  model_doc_link: '',
  config_keys: [],
  allows_unlisted_models: true,
};

describe('model vision resolution', () => {
  it('matches exact known model metadata', () => {
    expect(modelSupportsVision(metadata, 'gpt-5.5')).toBe(true);
    expect(modelSupportsVision(metadata, 'text-only')).toBe(false);
  });

  it('matches dated deployments to their vision-capable family', () => {
    expect(modelSupportsVision(metadata, 'gpt-5.5-2026-04-24')).toBe(true);
    expect(resolveKnownModelInfo(metadata, 'gpt-5.5-2026-04-24')?.name).toBe('gpt-5.5');
  });

  it('does not infer vision for codex variants', () => {
    expect(modelSupportsVision(metadata, 'gpt-5.3-codex-2026-01-01')).toBe(false);
  });

  it.each([
    ['anthropic/claude-sonnet-4.6', 'claude-sonnet-4-6'],
    ['anthropic/claude-sonnet-4-6', 'claude-sonnet-4-6'],
    ['databricks-claude-sonnet-4-6', 'claude-sonnet-4-6'],
    ['us.anthropic.claude-sonnet-4-6', 'claude-sonnet-4-6'],
    ['us.anthropic.claude-opus-4-6-v1', 'claude-opus-4-6'],
    ['us.anthropic.claude-opus-4-6-v1:0', 'claude-opus-4-6'],
    ['claude-sonnet-4@20250514', 'claude-sonnet-4-20250514'],
  ])('matches Claude alias %s to known model %s', (modelName, expectedKnownModel) => {
    expect(resolveKnownModelInfo(claudeMetadata, modelName)?.name).toBe(expectedKnownModel);
    expect(modelSupportsVision(claudeMetadata, modelName)).toBe(true);
  });

  it('does not overmatch similarly named Claude models', () => {
    expect(resolveKnownModelInfo(claudeMetadata, 'claude-sonnet-4-60')).toBeUndefined();
    expect(modelSupportsVision(claudeMetadata, 'claude-sonnet-4-60')).toBe(false);
  });

  it('resolves structured input MIME types through provider aliases', () => {
    expect(
      modelSupportedInputMimeTypes(claudeMetadata, 'us.anthropic.claude-opus-4-6-v1')
    ).toEqual(['image/png', 'image/jpeg', 'image/jpg', 'image/gif', 'image/webp']);
  });
});
