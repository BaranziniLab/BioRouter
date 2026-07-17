import React from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import MCPUIResourceRenderer from './MCPUIResourceRenderer';
import { ThemeProvider } from '../contexts/ThemeContext';
import type { EmbeddedResource } from '../api';
import { UIResourceRenderer } from '@mcp-ui/client';

vi.mock('@mcp-ui/client', () => ({
  UIResourceRenderer: vi.fn((props: { htmlProps: { style: React.CSSProperties } }) =>
    React.createElement('iframe', {
      'data-testid': 'mcp-ui-frame',
      title: 'mock mcp ui frame',
      style: props.htmlProps.style,
    })
  ),
}));

function renderSubject(
  uri = 'ui://chart/visualization',
  extraProps: { sessionId?: string; appendPromptToChat?: (value: string) => void } = {}
) {
  const html = '<!doctype html><html><body><h1>Chart</h1></body></html>';
  const blob = window.btoa(html);
  const openArtifactWindow = vi.fn().mockResolvedValue(undefined);

  Object.defineProperty(window, 'electron', {
    configurable: true,
    value: {
      getBiorouterdHostPort: vi.fn().mockResolvedValue('http://localhost:8765'),
      getSecretKey: vi.fn().mockResolvedValue('secret'),
      openArtifactWindow,
      on: vi.fn().mockReturnValue(() => undefined),
    },
  });

  if (!window.matchMedia) {
    Object.defineProperty(window, 'matchMedia', {
      configurable: true,
      value: vi.fn().mockReturnValue({
        matches: false,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      }),
    });
  }

  const content = {
    type: 'resource',
    resource: {
      uri,
      mimeType: 'text/html',
      blob,
    },
  } as EmbeddedResource & { type: 'resource' };

  const result = render(
    <ThemeProvider>
      <MCPUIResourceRenderer content={content} {...extraProps} />
    </ThemeProvider>
  );

  return { ...result, html, openArtifactWindow };
}

describe('MCPUIResourceRenderer', () => {
  it('renders MCP UI resources as an inline visualization instead of a boxed iframe card', () => {
    const { container } = renderSubject();
    const wrapper = container.firstElementChild as HTMLElement;

    expect(wrapper).toHaveClass('bg-transparent');
    expect(wrapper.className).not.toContain('border ');
    expect(wrapper.className).not.toContain('p-3');
    expect(screen.queryByText('Chart Visualization')).not.toBeInTheDocument();
    expect(screen.queryByText('Expand')).not.toBeInTheDocument();
    expect(UIResourceRenderer).toHaveBeenCalledWith(
      expect.objectContaining({
        htmlProps: expect.objectContaining({
          style: expect.objectContaining({ border: 'none', width: '100%' }),
        }),
      }),
      undefined
    );
  });

  it('keeps an icon-only expand affordance that opens the artifact window', async () => {
    const { html, openArtifactWindow } = renderSubject();

    fireEvent.click(screen.getByRole('button', { name: /open chart visualization/i }));

    await waitFor(() => {
      expect(openArtifactWindow).toHaveBeenCalledWith(
        expect.objectContaining({
          html,
          title: 'Chart Visualization',
          width: 1100,
          height: 820,
          theme: 'light',
        })
      );
    });
  });

  it('does not offer a delete control for agent-drafter apps in chat', () => {
    // Deleting an app is destructive and belongs in the Applications tab, not on
    // an in-chat artifact card where a stray click removes files from disk.
    renderSubject('ui://agent-drafter/researcher-impact-dashboard');

    expect(
      screen.queryByRole('button', { name: /delete researcher-impact-dashboard/i })
    ).not.toBeInTheDocument();
    expect(screen.queryByTitle('Delete application')).not.toBeInTheDocument();
    // The open/expand affordance is still there.
    expect(
      screen.getByRole('button', { name: /^Open .+ Researcher Impact Dashboard/i })
    ).toBeInTheDocument();
  });

  it('opens MCP HTML artifacts in the side viewer when an artifact handler is provided', async () => {
    const onOpenArtifact = vi.fn();
    const html = '<!doctype html><html><body><h1>Chart</h1></body></html>';
    const blob = btoa(html);

    Object.defineProperty(window, 'electron', {
      configurable: true,
      value: {
        getBiorouterdHostPort: vi.fn().mockResolvedValue('http://localhost:8765'),
        getSecretKey: vi.fn().mockResolvedValue('secret'),
        openArtifactWindow: vi.fn().mockResolvedValue(undefined),
        on: vi.fn().mockReturnValue(() => undefined),
      },
    });

    render(
      <ThemeProvider>
        <MCPUIResourceRenderer
          content={
            {
              type: 'resource',
              resource: {
                uri: 'ui://chart/visualization',
                mimeType: 'text/html',
                blob,
              },
            } as EmbeddedResource & { type: 'resource' }
          }
          onOpenArtifact={onOpenArtifact}
        />
      </ThemeProvider>
    );

    fireEvent.click(screen.getByRole('button', { name: /open chart visualization/i }));

    expect(onOpenArtifact).toHaveBeenCalledWith(
      expect.objectContaining({
        kind: 'html',
        title: 'Chart Visualization',
        html,
      })
    );
  });

  // This renderer lives INSIDE a chat, but 'scroll-chat-to-bottom' is a window
  // broadcast heard by every mounted BaseChat. Without a sessionId on the
  // payload, a prompt action in chat A scrolls chat B.
  it('scopes its scroll-to-bottom request to the chat it is rendered inside', async () => {
    const dispatch = vi.spyOn(window, 'dispatchEvent');
    const appendPromptToChat = vi.fn();
    renderSubject('ui://chart/visualization', {
      sessionId: 'session-aaa',
      appendPromptToChat,
    });

    const calls = vi.mocked(UIResourceRenderer).mock.calls;
    const { onUIAction } = calls[calls.length - 1][0] as unknown as {
      onUIAction: (e: unknown) => Promise<unknown>;
    };
    await onUIAction({ type: 'prompt', payload: { prompt: 'plot residuals' } });

    expect(appendPromptToChat).toHaveBeenCalledWith('plot residuals');
    const scrollEvent = dispatch.mock.calls
      .map((c) => c[0] as CustomEvent)
      .find((e) => e?.type === 'scroll-chat-to-bottom');
    expect(scrollEvent?.detail).toEqual({ sessionId: 'session-aaa' });
    dispatch.mockRestore();
  });
});
