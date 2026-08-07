/**
 * BR-71 Task 37: the `sub` badge the strip renders for a subagent tab.
 *
 * Its own file because the strip's other three suites all render bare — with no
 * `tabAnnotations` at all — which is exactly the contract that lets them keep
 * passing unedited, and exactly why none of them ever takes this branch. Without
 * these tests the badge could be wired to the wrong key, or never render, and
 * ship green.
 */
import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/react';
import { ChatTabStrip, ChatTabStripProps } from './ChatTabStrip';
import { ChatTab } from './chatGroupsTypes';
import type { TabAnnotation } from './workspaceCommandPlanner';

function tab(over: Partial<ChatTab> = {}): ChatTab {
  return {
    tabId: 'tab-1',
    sessionId: 's1',
    title: 'Cohort query',
    userSetName: false,
    ...over,
  };
}

function renderStrip(over: Partial<ChatTabStripProps> = {}) {
  const props: ChatTabStripProps = {
    tabs: [tab()],
    activeTabId: 'tab-1',
    runningSessionIds: [],
    onSelect: vi.fn(),
    onClose: vi.fn(),
    onReorder: vi.fn(),
    reserveTitlebar: false,
    isCompactSidebarOverlayOpen: false,
    ...over,
  };
  return render(<ChatTabStrip {...props} />);
}

/**
 * The subagent marking, scoped to one tab — never a document-wide search.
 *
 * ⚠ It used to be a `sub` TEXT chip beside the title. It is now the tab's
 * leading glyph: a sub-agent reads as a robot in the same slot every other kind
 * uses, instead of a word competing with the title for width. The lookup is by
 * the glyph's kind, so this suite still fails if the annotation is wired to the
 * wrong key or dropped.
 */
function badgeIn(container: HTMLElement, tabId: string): HTMLElement | null {
  const node = container.querySelector(`[data-tab-id="${tabId}"]`) as HTMLElement;
  const glyph = node.querySelector('[data-chat-kind]');
  return glyph?.getAttribute('data-chat-kind') === 'subagent' ? (glyph as HTMLElement) : null;
}

const subagent: TabAnnotation = { badge: 'subagent', parentSessionId: 'parent-1' };

describe('ChatTabStrip — the subagent badge', () => {
  it('marks a tab the workspace annotated as a subagent', () => {
    const { container } = renderStrip({ tabAnnotations: { s1: subagent } });
    expect(badgeIn(container, 'tab-1')).not.toBeNull();
  });

  it('is keyed by SESSION id, not tab id', () => {
    // The two ids are different strings on purpose. Keying the lookup by
    // `tab.tabId` would badge nothing here and everything in the test above,
    // which is why that test alone cannot catch the mistake.
    const { container } = renderStrip({ tabAnnotations: { 'tab-1': subagent } });
    expect(badgeIn(container, 'tab-1')).toBeNull();
  });

  it('badges only the annotated tab, not its neighbours', () => {
    const { container } = renderStrip({
      tabs: [tab(), tab({ tabId: 'tab-2', sessionId: 's2', title: 'Child' })],
      tabAnnotations: { s2: subagent },
    });
    expect(badgeIn(container, 'tab-1')).toBeNull();
    expect(badgeIn(container, 'tab-2')).not.toBeNull();
  });

  it('ignores an annotation that carries some other badge', () => {
    // `TabAnnotation.badge` is a free-form string; only 'subagent' means this.
    const { container } = renderStrip({ tabAnnotations: { s1: { badge: 'pinned' } } });
    expect(badgeIn(container, 'tab-1')).toBeNull();
  });

  it('ignores an annotation that carries no badge at all', () => {
    // `annotate_tab` always writes an annotation object, with `badge` simply
    // undefined when the command omitted it (workspaceCommandPlanner).
    const { container } = renderStrip({ tabAnnotations: { s1: { parentSessionId: 'p' } } });
    expect(badgeIn(container, 'tab-1')).toBeNull();
  });

  it('renders with no annotations prop at all', () => {
    // The optional-with-default contract, asserted rather than assumed: the
    // shell's own suites mock useChatGroups with stubs that have no
    // `tabAnnotations`, so the prop genuinely arrives undefined in production
    // code paths, and a required prop would throw on every tab.
    const { container } = renderStrip();
    expect(container.querySelector('[data-tab-id="tab-1"]')).toBeTruthy();
    expect(badgeIn(container, 'tab-1')).toBeNull();
  });
});
