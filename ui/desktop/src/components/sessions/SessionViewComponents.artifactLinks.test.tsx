import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { Message } from '../../api';
import { SessionMessages } from './SessionViewComponents';

function assistant(id: string, text: string, fromSessionId?: string): Message {
  return {
    id,
    role: 'assistant',
    created: 1,
    metadata: {
      userVisible: true,
      agentVisible: true,
      ...(fromSessionId ? { provenance: { kind: 'agent_injection' as const, fromSessionId } } : {}),
    },
    content: [{ type: 'text', text }],
  };
}

function renderMessages(messages: Message[], onOpenArtifact = vi.fn()) {
  render(
    <SessionMessages
      messages={messages}
      sessionId="saved-session"
      isLoading={false}
      error={null}
      onRetry={vi.fn()}
      onOpenArtifact={onOpenArtifact}
      workingDir="/work"
    />
  );
  return onOpenArtifact;
}

describe('file-link reliability: read-only transcript provenance', () => {
  it('opens a unique earlier same-session basename at its proven path', async () => {
    const onOpenArtifact = renderMessages([
      assistant('prior', 'Created `/elsewhere/results/report.md`.'),
      assistant('current', '[Open report](report.md)'),
    ]);
    fireEvent.click(await screen.findByRole('button', { name: 'Open report' }));
    expect(onOpenArtifact).toHaveBeenCalledWith({
      kind: 'file',
      title: 'report.md',
      path: '/elsewhere/results/report.md',
    });
  });

  it('refuses an ambiguous basename from two earlier files', async () => {
    const onOpenArtifact = renderMessages([
      assistant('prior', 'Compare `/one/report.md` and `/two/report.md`.'),
      assistant('current', '[Open report](report.md)'),
    ]);
    expect(await screen.findByText('Open report')).toBeVisible();
    expect(screen.queryByRole('button', { name: 'Open report' })).not.toBeInTheDocument();
    expect(onOpenArtifact).not.toHaveBeenCalled();
  });

  it('does not reuse a foreign-origin path as local provenance', async () => {
    const onOpenArtifact = renderMessages([
      assistant('foreign', 'Created `/foreign/report.md`.', 'foreign-session'),
      assistant('current', '[Open report](report.md)'),
    ]);
    fireEvent.click(await screen.findByRole('button', { name: 'Open report' }));
    expect(onOpenArtifact).toHaveBeenCalledWith({
      kind: 'file',
      title: 'report.md',
      path: '/work/report.md',
    });
  });

  it('keeps an explicit absolute path unchanged', async () => {
    const onOpenArtifact = renderMessages([
      assistant('prior', 'Created `/one/report.md`.'),
      assistant('current', '[Open report](/explicit/report.md)'),
    ]);
    fireEvent.click(await screen.findByRole('button', { name: 'Open report' }));
    expect(onOpenArtifact).toHaveBeenCalledWith({
      kind: 'file',
      title: 'report.md',
      path: '/explicit/report.md',
    });
  });
});
