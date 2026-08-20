import { describe, expect, it } from 'vitest';
import {
  ingestModelBlockedByProvider,
  providerCanRunIngest,
  resolveIngestModel,
} from './resolveIngestModel';

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

describe('providers a knowledge macro cannot drive', () => {
  it('names the coding-agent providers and nothing else', () => {
    // These two accept `tools` on `complete_with_model` and drop them: their
    // tool surface only arrives over the MCP bridge the chat-turn loop sets up,
    // and a macro calls the provider directly.
    expect(providerCanRunIngest('claude_code')).toBe(false);
    expect(providerCanRunIngest('codex')).toBe(false);
    // Everything else is fine, including the vendors those two wrap — it is the
    // subprocess-CLI provider that cannot carry tools here, not Anthropic or
    // OpenAI.
    expect(providerCanRunIngest('anthropic')).toBe(true);
    expect(providerCanRunIngest('openai')).toBe(true);
    expect(providerCanRunIngest('ollama')).toBe(true);
    expect(providerCanRunIngest('versa_azure')).toBe(true);
  });

  it('does not preselect the chat model when the chat model is one of them', () => {
    // The user's composer is on Claude Code and it works there. Digestion is a
    // different path, and inheriting the chat model onto it buys a full model
    // run that writes nothing.
    expect(resolveIngestModel(null, 'claude_code', 'opus-5')).toBeNull();
    expect(resolveIngestModel(null, 'codex', 'gpt-5.4-codex')).toBeNull();
  });

  it("skips a base's stored default rather than honouring one that cannot work", () => {
    // A `default_model` naming one of these could only have been saved before
    // the exclusion existed. Honouring it would dispatch exactly the run this
    // guard is here to prevent, so the app configuration takes over.
    expect(
      resolveIngestModel({ provider: 'claude_code', model: 'opus-5' }, 'ollama', 'qwen3.6:latest')
    ).toEqual({ provider: 'ollama', model: 'qwen3.6:latest' });
  });

  it('reports the refusal separately from an empty configuration', () => {
    // The two nulls mean different things to the user: one is "go and set a
    // model up", the other is "the one you have cannot do this job". A picker
    // that cannot tell them apart sends the second user to Settings for nothing.
    expect(ingestModelBlockedByProvider(null, 'claude_code', 'opus-5')).toBe(true);
    expect(
      ingestModelBlockedByProvider({ provider: 'codex', model: 'gpt-5.4-codex' }, null, null)
    ).toBe(true);

    expect(ingestModelBlockedByProvider(null, null, null)).toBe(false);
    expect(ingestModelBlockedByProvider(null, 'ollama', 'qwen3.6:latest')).toBe(false);
    // A half-populated candidate is not a refusal — nothing was rejected, there
    // was simply never a usable pair.
    expect(ingestModelBlockedByProvider(null, 'claude_code', '')).toBe(false);
  });
});
