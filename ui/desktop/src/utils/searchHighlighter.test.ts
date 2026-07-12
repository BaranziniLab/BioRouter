import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { SearchHighlighter } from './searchHighlighter';

class ResizeObserverStub {
  observe = vi.fn();
  disconnect = vi.fn();
}

describe('SearchHighlighter', () => {
  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', ResizeObserverStub);
    document.body.innerHTML = `
      <div data-search-scroll-area>
        <div data-radix-scroll-area-viewport>
          <div id="search-content">alpha beta</div>
        </div>
      </div>
    `;
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it('removes its scroll listener when destroyed', () => {
    const content = document.getElementById('search-content') as HTMLDivElement;
    const viewport = document.querySelector('[data-radix-scroll-area-viewport]') as HTMLDivElement;
    const addEventListener = vi.spyOn(viewport, 'addEventListener');
    const removeEventListener = vi.spyOn(viewport, 'removeEventListener');

    const highlighter = new SearchHighlighter(content);
    const scrollRegistration = addEventListener.mock.calls.find(([type]) => type === 'scroll');
    expect(scrollRegistration).toBeDefined();

    highlighter.destroy();

    expect(removeEventListener).toHaveBeenCalledWith('scroll', scrollRegistration?.[1]);
  });
});
