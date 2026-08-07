import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { MODAL_SIZE, ModalShell, type ModalPurpose } from './ModalShell';

afterEach(cleanup);

function open(props: Partial<React.ComponentProps<typeof ModalShell>> = {}) {
  const onOpenChange = vi.fn();
  render(
    <ModalShell open onOpenChange={onOpenChange} title="Do the thing" {...props}>
      <p>body</p>
    </ModalShell>
  );
  return { onOpenChange };
}

const surface = () => document.querySelector('[data-slot="dialog-content"]')!;

describe('ModalShell — the structural guarantee', () => {
  // A modal whose content loses to its own scrim covers the whole app and reads
  // as a freeze; the app has shipped that bug. `.biorouter-modal-surface` is the
  // unlayered `z-index: var(--z-modal)` floor that makes it unreachable, and it
  // arrives only because the shell renders through the Radix `DialogContent`.
  // Nothing here may paint its own overlay or set its own z-index.
  it('renders on the primitive that carries the z-index floor, and sets no z-index of its own', () => {
    open();
    const content = surface();
    expect(content).toHaveClass('biorouter-modal-surface');
    // The one z-* class allowed is the primitive's own --z-modal.
    const zClasses = Array.from(content.classList).filter((c) => /^z-/.test(c));
    expect(zClasses).toEqual(['z-[var(--z-modal)]']);

    const overlay = document.querySelector('[data-slot="dialog-overlay"]')!;
    expect(overlay).toHaveClass('z-[var(--z-overlay)]');
  });
});

describe('ModalShell — the size scale', () => {
  it.each(Object.entries(MODAL_SIZE))('%s maps to exactly one width', (size, className) => {
    open({ size: size as keyof typeof MODAL_SIZE });
    const widths = Array.from(surface().classList).filter((c) => c.includes('max-w'));
    // The primitive's default `sm:max-w-lg` must have been merged away, leaving
    // the scale's width plus the unprefixed small-screen clamp.
    expect(widths).toContain(className);
    expect(widths).not.toContain('sm:max-w-lg');
  });
});

describe('ModalShell — the purpose axis', () => {
  it('info dismisses on Escape', async () => {
    const user = userEvent.setup();
    const { onOpenChange } = open({ purpose: 'info' });
    await user.keyboard('{Escape}');
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  // The bug the axis exists to prevent: a half-filled form thrown away by a
  // misclick on the scrim.
  it('form ignores a backdrop click but keeps its Escape route', async () => {
    const user = userEvent.setup();
    const { onOpenChange } = open({ purpose: 'form' });

    await user.click(document.querySelector('[data-slot="dialog-overlay"]')!);
    expect(onOpenChange).not.toHaveBeenCalled();

    await user.keyboard('{Escape}');
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it('required offers no close affordance and ignores Escape', async () => {
    const user = userEvent.setup();
    const { onOpenChange } = open({ purpose: 'required' });

    expect(screen.queryByRole('button', { name: 'Close' })).not.toBeInTheDocument();
    await user.keyboard('{Escape}');
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it.each<[ModalPurpose, boolean]>([
    ['info', true],
    ['form', true],
    ['required', false],
  ])('%s renders the single × exactly %s times', (purpose, shown) => {
    open({ purpose });
    // ONE close affordance: the primitive's ×, never a second hand-rolled one.
    expect(screen.queryAllByRole('button', { name: 'Close' })).toHaveLength(shown ? 1 : 0);
  });
});
