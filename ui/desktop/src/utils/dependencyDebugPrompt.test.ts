import { describe, it, expect } from 'vitest';
import {
  buildDependencyDebugPrompt,
  truncateOutput,
  verifyCommand,
  debugSessionTitle,
  MAX_OUTPUT_CHARS,
  type DependencyFailure,
} from './dependencyDebugPrompt';

const base: DependencyFailure = {
  kind: 'dependency',
  name: 'uv',
  displayName: 'uv (Python package manager)',
  command: 'curl -LsSf https://astral.sh/uv/install.sh | sh',
  exitCode: 1,
  output: 'curl: (6) Could not resolve host: astral.sh',
  error: 'Process exited with code 1',
  downloadUrl: 'https://docs.astral.sh/uv/getting-started/installation/',
  environment: {
    platform: 'darwin',
    arch: 'arm64',
    appVersion: '1.89.1',
    augmentedPath: '/opt/homebrew/bin:/usr/bin',
    inheritedPath: '/usr/bin',
    homedir: '/Users/example',
  },
};

describe('buildDependencyDebugPrompt', () => {
  it('carries the evidence the agent needs to skip a round of questions', () => {
    const p = buildDependencyDebugPrompt(base);
    expect(p).toContain('uv (Python package manager)');
    expect(p).toContain('curl -LsSf https://astral.sh/uv/install.sh | sh');
    expect(p).toContain('Exit code: 1');
    expect(p).toContain('Could not resolve host');
    expect(p).toContain('darwin');
    expect(p).toContain('1.89.1');
  });

  it('puts the evidence above the instructions', () => {
    const p = buildDependencyDebugPrompt(base);
    expect(p.indexOf('Could not resolve host')).toBeLessThan(p.indexOf('What I need from you'));
  });

  it('names the verification command so the fix is proven, not asserted', () => {
    expect(buildDependencyDebugPrompt(base)).toContain('uv --version');
    expect(verifyCommand({ ...base, kind: 'cli', name: 'biorouter' })).toBe('biorouter --version');
    expect(verifyCommand({ ...base, kind: 'extension' })).toBe('uv sync');
  });

  it('asks before sudo, profile edits and removals', () => {
    const p = buildDependencyDebugPrompt(base);
    expect(p).toContain('Ask me first');
    expect(p).toContain('sudo');
    expect(p).toContain('shell profile');
  });

  it('flags the launchd-PATH trap when the two PATHs differ', () => {
    expect(buildDependencyDebugPrompt(base)).toContain('does not');
    expect(buildDependencyDebugPrompt(base)).toContain('.zshrc');
  });

  it('omits the PATH note when both PATHs agree', () => {
    const same = '/usr/bin:/bin';
    const p = buildDependencyDebugPrompt({
      ...base,
      environment: { ...base.environment, augmentedPath: same, inheritedPath: same },
    });
    expect(p).not.toContain('.zshrc');
  });

  it('still produces a usable briefing with almost nothing to go on', () => {
    const p = buildDependencyDebugPrompt({ kind: 'dependency', name: 'git' });
    expect(p).toContain('git');
    expect(p).toContain('What I need from you');
    expect(p).not.toContain('undefined');
    expect(p).not.toContain('Exit code');
  });

  it('frames each failure kind as what it actually is', () => {
    expect(buildDependencyDebugPrompt({ ...base, kind: 'cli' })).toContain('command-line tool');
    expect(buildDependencyDebugPrompt({ ...base, kind: 'extension' })).toContain('extension');
    expect(buildDependencyDebugPrompt({ ...base, kind: 'script' })).toContain('setup script');
  });

  it('mentions elevated privileges only when the install needs them', () => {
    expect(buildDependencyDebugPrompt({ ...base, requiresSudo: true })).toContain(
      'administrator privileges'
    );
    expect(buildDependencyDebugPrompt(base)).not.toContain('administrator privileges');
  });

  it('does not let output containing a code fence break out of its block', () => {
    const p = buildDependencyDebugPrompt({
      ...base,
      output: 'error in:\n```\nsome code\n```\nfailed',
    });
    // The opening delimiter must be longer than any run inside the body.
    expect(p).toContain('````');
    const opening = p.slice(p.indexOf('Output from the failed command'));
    expect(opening.split('````').length).toBeGreaterThanOrEqual(3);
  });
});

describe('truncateOutput', () => {
  it('keeps short output verbatim', () => {
    expect(truncateOutput('boom')).toBe('boom');
  });

  it('keeps the tail, where the failure is', () => {
    const out = 'x'.repeat(MAX_OUTPUT_CHARS * 2) + 'THE_ACTUAL_ERROR';
    const t = truncateOutput(out);
    expect(t).toContain('THE_ACTUAL_ERROR');
    expect(t).toContain('earlier characters omitted');
    expect(t.length).toBeLessThan(out.length);
  });

  it('never grows output that is only just over the cap', () => {
    const out = 'z'.repeat(MAX_OUTPUT_CHARS + 16);
    expect(truncateOutput(out).length).toBeLessThanOrEqual(out.length);
  });

  it('bounds a pathological log rather than pasting megabytes into the composer', () => {
    const t = truncateOutput('y'.repeat(5_000_000));
    expect(t.length).toBeLessThan(MAX_OUTPUT_CHARS + 200);
  });
});

describe('debugSessionTitle', () => {
  it('names the thing being fixed', () => {
    expect(debugSessionTitle(base)).toBe('Fix uv (Python package manager)');
    expect(debugSessionTitle({ kind: 'dependency', name: 'git' })).toBe('Fix git');
  });
});
