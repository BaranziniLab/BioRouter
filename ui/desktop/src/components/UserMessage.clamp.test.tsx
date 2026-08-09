/**
 * The long-message clamp, at the component level.
 *
 * The threshold and the count are unit-tested as pure functions in
 * `utils/messageClamp.test.ts`; what is left for here is the part that only
 * exists once a component is mounted — that the cap and the fade land on the
 * div that carries the fill, that the cap is REMOVED (not merely raised) on
 * expand, that expansion survives a re-render, and that Copy is unaffected.
 *
 * Every fixture below is genuinely long. A short-line fixture cannot catch a
 * clamping bug: it passes against a clamp that is wired to the wrong element,
 * to the wrong threshold, or to nothing at all.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import UserMessage from './UserMessage';
import type { Message } from '../api';
import { CLAMP_EXPAND_MS, CLAMP_MAX_HEIGHT_PX } from '../utils/messageClamp';

/** A 400-line traceback with one 900-character unbroken token in the middle. */
const LONG_PASTE = [
  'Traceback (most recent call last):',
  ...Array.from(
    { length: 200 },
    (_, i) => `  File "run_pipeline.py", line ${i}, in main\n    adata = qc.filter_cells(adata)`
  ),
  `ValueError: ${'0123456789'.repeat(90)}`,
].join('\n');

/** ~180 words of running prose on a handful of long lines — no hard wrapping. */
const LONG_PROSE = Array.from(
  { length: 3 },
  () =>
    'I want to redo the MS cohort analysis end to end but with the QC thresholds from the 2025 paper rather than the ones currently in the pipeline, filtering cells at 200 genes instead of 500 and keeping the mitochondrial cutoff where it is, then comparing the resulting cluster assignments against the ones we generated last week.'
).join('\n\n');

const userMessage = (text: string, id = 'clamp-1'): Message => ({
  id,
  role: 'user',
  created: 1,
  content: [{ type: 'text', text }],
  metadata: { userVisible: true, agentVisible: true },
});

/** The div the toggle controls — the one that must carry the fill and the cap. */
const bubbleFor = (toggle: HTMLElement) =>
  document.getElementById(toggle.getAttribute('aria-controls') ?? '');

const toggle = () => screen.getByRole('button', { name: /show (more|less)/i });

beforeEach(() => {
  Object.assign(window, { electron: { logInfo: vi.fn() } });
});

afterEach(() => {
  vi.useRealTimers();
});

describe('when the message is short', () => {
  it('offers no control and caps nothing', () => {
    render(<UserMessage message={userMessage('a short question')} />);

    expect(screen.queryByRole('button', { name: /show more/i })).toBeNull();
    expect(screen.queryByTestId('message-clamp-count')).toBeNull();
    // Nothing in the bubble is clipped, so nothing can hide.
    expect(document.querySelector('.overflow-hidden')).toBeNull();
  });
});

describe('when the message is long', () => {
  it('caps and fades the div that carries the fill, not the text child', () => {
    // This is the bug the clamp is easiest to write: put max-height on the text
    // and the tint keeps its full height, leaving an empty coloured tail under a
    // gradient that is fading to a colour nothing is painting.
    render(<UserMessage message={userMessage(LONG_PASTE)} />);

    const bubble = bubbleFor(toggle());
    expect(bubble).not.toBeNull();
    expect(bubble!.className).toContain('bg-background-medium');
    expect(bubble!.className).toContain('overflow-hidden');
    expect(bubble!.className).toContain('var(--background-medium)');
    expect(bubble!.style.maxHeight).toBe(`${CLAMP_MAX_HEIGHT_PX}px`);
  });

  it('keeps every line in the DOM — clipped, never unmounted', () => {
    // A collapsed message is still readable by a screen reader and still
    // findable by the transcript's Cmd-F, because the text was never removed.
    render(<UserMessage message={userMessage(LONG_PASTE)} />);

    const bubble = bubbleFor(toggle())!;
    expect(bubble.textContent).toContain('Traceback (most recent call last):');
    expect(bubble.textContent).toContain('line 199');
    expect(bubble.textContent).toContain('0123456789'.repeat(90));
  });

  it('carries the count, in lines and bytes for a line-structured message', () => {
    render(<UserMessage message={userMessage(LONG_PASTE)} />);
    expect(screen.getByTestId('message-clamp-count').textContent).toMatch(
      /^402 lines · \d+(\.\d)? KB$/
    );
  });

  it('carries the count, in words for running prose', () => {
    render(<UserMessage message={userMessage(LONG_PROSE)} />);
    expect(screen.getByTestId('message-clamp-count').textContent).toMatch(/^\d+ words$/);
  });

  it('clamps typed prose, not just pastes', () => {
    // The clamp is length-gated, full stop. A version of this feature that only
    // folded pasted matter would pass every other test in this file.
    render(<UserMessage message={userMessage(LONG_PROSE)} />);
    expect(bubbleFor(toggle())!.style.maxHeight).toBe(`${CLAMP_MAX_HEIGHT_PX}px`);
  });
});

describe('expanding', () => {
  it('removes the cap entirely rather than raising it', async () => {
    // A literal like the design specimen's `max-height: 1200px` would clip a
    // 5000-line paste — the exact message this feature exists for. Expanded
    // means uncapped.
    render(<UserMessage message={userMessage(LONG_PASTE)} />);
    fireEvent.click(toggle());

    await waitFor(() => {
      const bubble = bubbleFor(screen.getByRole('button', { name: /show less/i }))!;
      expect(bubble.style.maxHeight).toBe('');
      expect(bubble.className).not.toContain('overflow-hidden');
      // The fade goes with the cap: there is nothing left to fade to.
      expect(bubble.className).not.toContain('var(--background-medium)');
    });
  });

  it('animates from the measured height before releasing to automatic sizing', async () => {
    // jsdom does no layout, so scrollHeight is 0 and the component opens
    // directly. Stub a real measurement to exercise the intermediate state that
    // makes the growth animate at all.
    const scrollHeight = vi
      .spyOn(HTMLElement.prototype, 'scrollHeight', 'get')
      .mockReturnValue(4820);
    vi.useFakeTimers();
    try {
      render(<UserMessage message={userMessage(LONG_PASTE)} />);
      fireEvent.click(toggle());

      const expanding = bubbleFor(screen.getByRole('button', { name: /show less/i }))!;
      expect(expanding.style.maxHeight).toBe('4820px');
      expect(expanding.className).toContain('transition-[max-height]');
      expect(expanding.className).toContain('duration-[var(--dur-med)]');

      act(() => {
        vi.advanceTimersByTime(CLAMP_EXPAND_MS);
      });

      const open = bubbleFor(screen.getByRole('button', { name: /show less/i }))!;
      expect(open.style.maxHeight).toBe('');
      expect(open.className).not.toContain('transition-[max-height]');
    } finally {
      scrollHeight.mockRestore();
    }
  });

  it('stays expanded across a re-render', async () => {
    // "Expanded is sticky per message and never re-collapses while you are
    // reading" — and the transcript re-renders this component once per streamed
    // token of the reply below it.
    const { rerender } = render(<UserMessage message={userMessage(LONG_PASTE)} />);
    fireEvent.click(toggle());
    await screen.findByRole('button', { name: /show less/i });

    for (let i = 0; i < 3; i++) {
      rerender(<UserMessage message={userMessage(LONG_PASTE)} onMessageUpdate={vi.fn()} />);
    }

    expect(screen.getByRole('button', { name: /show less/i })).toBeTruthy();
    expect(bubbleFor(screen.getByRole('button', { name: /show less/i }))!.style.maxHeight).toBe('');
  });

  it('collapses again on demand, instantly', async () => {
    render(<UserMessage message={userMessage(LONG_PASTE)} />);
    fireEvent.click(toggle());
    fireEvent.click(await screen.findByRole('button', { name: /show less/i }));

    const bubble = bubbleFor(screen.getByRole('button', { name: /show more/i }))!;
    expect(bubble.style.maxHeight).toBe(`${CLAMP_MAX_HEIGHT_PX}px`);
    // No transition class on the way down: the design specifies collapse as
    // instant, "because you are moving away from it".
    expect(bubble.className).not.toContain('transition-[max-height]');
  });

  it('reports its state to assistive technology', () => {
    render(<UserMessage message={userMessage(LONG_PASTE)} />);
    expect(toggle()).toHaveAttribute('aria-expanded', 'false');
    fireEvent.click(toggle());
    expect(screen.getByRole('button', { name: /show less/i })).toHaveAttribute(
      'aria-expanded',
      'true'
    );
  });
});

describe('copy', () => {
  it('takes the whole message while it is still collapsed', async () => {
    // The clamp is a view state, never a content state.
    render(<UserMessage message={userMessage(LONG_PASTE)} />);
    expect(toggle()).toBeTruthy(); // still collapsed

    fireEvent.click(screen.getByRole('button', { name: /copy message/i }));

    await waitFor(() => {
      expect(navigator.clipboard.writeText).toHaveBeenCalledWith(LONG_PASTE);
    });
  });
});
