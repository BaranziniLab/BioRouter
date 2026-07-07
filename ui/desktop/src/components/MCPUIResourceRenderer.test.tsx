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

function renderSubject(uri = 'ui://chart/visualization') {
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
      <MCPUIResourceRenderer content={content} />
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
    expect(screen.queryByText('visualization')).not.toBeInTheDocument();
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

    fireEvent.click(screen.getByRole('button', { name: /open visualization/i }));

    await waitFor(() => {
      expect(openArtifactWindow).toHaveBeenCalledWith(
        expect.objectContaining({
          html,
          title: 'visualization',
          width: 1100,
          height: 820,
          theme: 'light',
        })
      );
    });
  });

  it('allows agent-drafter app resources to be deleted from chat after confirmation', async () => {
    const fetchMock = vi.fn().mockResolvedValue(new Response('{"ok":true}', { status: 200 }));
    vi.stubGlobal('fetch', fetchMock);
    renderSubject('ui://agent-drafter/researcher-impact-dashboard');

    fireEvent.click(screen.getByRole('button', { name: /delete researcher-impact-dashboard/i }));
    fireEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() => {
      expect(fetchMock).toHaveBeenCalledWith(
        'http://localhost/apps/researcher-impact-dashboard',
        expect.objectContaining({
          method: 'DELETE',
          headers: { 'X-Secret-Key': 'secret' },
        })
      );
    });
    expect(screen.getByText(/Application deleted:/)).toBeInTheDocument();
    expect(screen.getByText('researcher-impact-dashboard')).toBeInTheDocument();
  });
});
