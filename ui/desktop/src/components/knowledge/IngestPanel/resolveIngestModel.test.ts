import { describe, expect, it } from 'vitest';
import { resolveIngestModel } from './resolveIngestModel';

describe('resolveIngestModel', () => {
  it("uses the app's configured provider and model when the base has no default", () => {
    expect(resolveIngestModel(null, 'versa_azure', 'gpt-5.5-2026-04-24')).toEqual({
      provider: 'versa_azure',
      model: 'gpt-5.5-2026-04-24',
    });
  });

  it("prefers the knowledge base's own default model over the app configuration", () => {
    expect(
      resolveIngestModel(
        { provider: 'ollama', model: 'qwen3.6:latest' },
        'versa_azure',
        'gpt-5.5-2026-04-24'
      )
    ).toEqual({ provider: 'ollama', model: 'qwen3.6:latest' });
  });

  it('returns null rather than inventing a vendor when nothing resolves', () => {
    expect(resolveIngestModel(null, null, null)).toBeNull();
    expect(resolveIngestModel(undefined, undefined, undefined)).toBeNull();
  });

  it('ignores a half-populated source instead of emitting a partial ModelRef', () => {
    expect(resolveIngestModel(null, 'versa_azure', '')).toBeNull();
    expect(resolveIngestModel(null, '', 'gpt-5.5-2026-04-24')).toBeNull();
    expect(resolveIngestModel({ provider: 'ollama', model: '' }, 'versa_azure', 'gpt-5.5')).toEqual(
      {
        provider: 'versa_azure',
        model: 'gpt-5.5',
      }
    );
  });
});
