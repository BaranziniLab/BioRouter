import { describe, it, expect } from 'vitest';
import { stripExtensionSecrets, type Workflow } from './index';

function workflowWith(extensions: Workflow['extensions']): Workflow {
  return {
    version: '1.0.0',
    title: 'T',
    description: 'D',
    instructions: 'I',
    extensions,
  } as Workflow;
}

describe('stripExtensionSecrets', () => {
  it('removes literal env values, which are secrets, from a shareable workflow', () => {
    const workflow = workflowWith([
      {
        type: 'stdio',
        name: 'spoke',
        cmd: 'uvx',
        args: [],
        envs: { SPOKEAGENT_PASSCODE: 'super-secret' },
        env_keys: ['SPOKEAGENT_PASSCODE'],
        timeout: 300,
      },
    ] as unknown as Workflow['extensions']);

    const stripped = stripExtensionSecrets(workflow);
    const extension = stripped.extensions![0] as unknown as Record<string, unknown>;

    expect(extension.envs).toBeUndefined();
    // env_keys must survive: the recipient resolves them from their own keyring.
    expect(extension.env_keys).toEqual(['SPOKEAGENT_PASSCODE']);
    expect(JSON.stringify(stripped)).not.toContain('super-secret');
  });

  it('leaves the original workflow untouched', () => {
    const workflow = workflowWith([
      { type: 'stdio', name: 'a', cmd: 'x', args: [], envs: { K: 'v' } },
    ] as unknown as Workflow['extensions']);

    stripExtensionSecrets(workflow);

    const original = workflow.extensions![0] as unknown as Record<string, unknown>;
    expect(original.envs).toEqual({ K: 'v' });
  });

  it('passes through workflows with no extensions', () => {
    const workflow = workflowWith(undefined);
    expect(stripExtensionSecrets(workflow)).toBe(workflow);
    expect(stripExtensionSecrets(workflowWith([]))).toEqual(workflowWith([]));
  });

  it('leaves extensions that carry no envs alone', () => {
    const workflow = workflowWith([
      { type: 'builtin', name: 'developer' },
    ] as unknown as Workflow['extensions']);

    const stripped = stripExtensionSecrets(workflow);
    expect(stripped.extensions![0]).toEqual({ type: 'builtin', name: 'developer' });
  });
});
