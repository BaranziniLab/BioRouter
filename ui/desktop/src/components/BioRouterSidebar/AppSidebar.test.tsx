import { fireEvent, render, screen } from '@testing-library/react';
import { useState } from 'react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { SessionSummary } from '../../api';
import { ChatProvider } from '../../contexts/ChatContext';
import type { ChatType } from '../../types/chat';
import { SidebarProvider } from '../ui/sidebar';

const mocks = vi.hoisted(() => ({
  listApps: vi.fn(),
  listSessions: vi.fn(),
  listSidebarSessions: vi.fn(),
}));

vi.mock('../../api', () => ({
  listApps: mocks.listApps,
  listSessions: mocks.listSessions,
  listSidebarSessions: mocks.listSidebarSessions,
}));

vi.mock('../../hooks/chatStreamStore', () => ({
  useRunningChats: () => [],
}));

vi.mock('./SidebarUpdateButton', () => ({
  default: () => null,
}));

import AppSidebar from './AppSidebar';

const session: SessionSummary = {
  id: 'session-1',
  name: 'Inspect latest cohort',
  created_at: '2026-07-14T12:00:00.000Z',
  updated_at: '2026-07-15T12:00:00.000Z',
  working_dir: '/workspace/cohort-study',
  message_count: 4,
};

beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
});

beforeEach(() => {
  mocks.listApps.mockResolvedValue({ data: { apps: [] } });
  mocks.listSessions.mockResolvedValue({ data: { sessions: [session] } });
  mocks.listSidebarSessions.mockResolvedValue({
    data: { sessions: [session], has_more: false, next_offset: null },
  });
});

function SidebarHarness() {
  const location = useLocation();
  const [chat, setChat] = useState<ChatType>({
    sessionId: 'previous-session',
    name: 'Existing chat',
    messages: [],
    workflow: null,
  });

  return (
    <ChatProvider chat={chat} setChat={setChat}>
      <SidebarProvider>
        <AppSidebar currentPath={location.pathname} onSelectSession={vi.fn()} />
      </SidebarProvider>
      <output data-testid="location-state">{`${location.pathname}${location.search}`}</output>
      <output data-testid="chat-session">{chat.sessionId || 'empty'}</output>
      <output data-testid="route-state">{JSON.stringify(location.state)}</output>
    </ChatProvider>
  );
}

describe('AppSidebar chat navigation', () => {
  it('labels the standard navigation action New Session, creates an empty route, and keeps recent history one click away', async () => {
    render(
      <MemoryRouter initialEntries={['/pair?resumeSessionId=previous-session']}>
        <SidebarHarness />
      </MemoryRouter>
    );

    expect(screen.queryByTestId('sidebar-history-button')).toBeNull();
    const newSessionButton = screen.getByTestId('sidebar-new-session-button');
    const homeButton = screen.getByTestId('sidebar-home-button');
    const settingsButton = screen.getByTestId('sidebar-settings-button');
    const wordmark = screen.getByTestId('sidebar-biorouter-wordmark');
    const sidebarContent = document.querySelector('[data-sidebar="content"]');
    const footer = document.querySelector('[data-sidebar="footer"]');
    const primaryMenu = homeButton.closest('[data-sidebar="menu"]');

    expect(newSessionButton).toHaveTextContent('New Session');
    expect(newSessionButton).toHaveClass('h-8', 'w-full', 'px-3', 'py-2', 'text-sm');
    expect(newSessionButton).not.toHaveClass(
      'h-9',
      'border',
      'bg-background-default/55',
      'font-semibold'
    );
    expect(wordmark).toHaveTextContent('Biorouter');
    expect(wordmark).toHaveClass('h-8');
    expect(sidebarContent).toHaveClass('pt-10');
    expect(wordmark.compareDocumentPosition(newSessionButton)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING
    );
    expect(newSessionButton.compareDocumentPosition(homeButton)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING
    );
    expect(homeButton).toHaveClass('h-8', 'px-3', 'py-2', 'text-sm');
    expect(primaryMenu).toHaveClass('gap-0');
    expect(primaryMenu).toContainElement(newSessionButton);
    expect(homeButton).not.toHaveClass('text-text-muted');
    expect(footer).toContainElement(settingsButton);
    expect(settingsButton).toHaveClass('h-8', 'w-full', 'px-3', 'py-2', 'text-sm');
    expect(settingsButton).not.toHaveClass('text-text-muted');
    expect(screen.queryByTestId('sidebar-biorouter-mark')).not.toBeInTheDocument();
    expect(screen.getByTestId('sidebar-nav-divider')).toHaveClass('!w-8');
    expect(screen.getByTestId('sidebar-footer-divider')).toHaveClass('!w-8');
    expect(screen.getByText('Recents')).toBeInTheDocument();
    expect(screen.getByTestId('view-all-chat-history')).toBeInTheDocument();
    expect(await screen.findByTestId('recent-chat-session-1')).toBeInTheDocument();

    fireEvent.click(newSessionButton);
    expect(screen.getByTestId('location-state')).toHaveTextContent('/pair');
    expect(screen.getByTestId('route-state')).toHaveTextContent('newChat');
    expect(screen.getByTestId('chat-session')).toHaveTextContent('empty');

    fireEvent.click(screen.getByTestId('recent-chat-session-1'));
    expect(screen.getByTestId('location-state')).toHaveTextContent(
      '/pair?resumeSessionId=session-1'
    );

    fireEvent.click(screen.getByTestId('view-all-chat-history'));
    expect(screen.getByTestId('location-state')).toHaveTextContent('/sessions');

    fireEvent.click(settingsButton);
    expect(screen.getByTestId('location-state')).toHaveTextContent('/settings');
  });
});
