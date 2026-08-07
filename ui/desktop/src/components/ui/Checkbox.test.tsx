import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Checkbox } from './Checkbox';

// jsdom has no layout engine and never runs Tailwind, so it cannot see the
// thing this component exists to fix — a bare `<input type="checkbox">` painting
// as macOS system blue in light mode and a bare white square in dark. What it
// can pin is the contract that lets the app do the painting at all, and the one
// piece of imperative state the component has to write by hand.
describe('Checkbox', () => {
  it('keeps a real, labellable input and hands the painting to the app', () => {
    render(
      <label>
        <Checkbox defaultChecked />
        Show subagent runs
      </label>
    );
    const input = screen.getByLabelText(/show subagent runs/i);
    expect(input).toHaveAttribute('type', 'checkbox');
    // Not `hidden`, not a div with a role: the native control stays in the tree
    // and in the tab order, so keyboard, form participation and the label
    // association all keep working. `sr-only` is only what stops the OS drawing
    // a second, un-themeable box on top of ours.
    expect(input).toHaveClass('sr-only');
    expect((input as HTMLInputElement).checked).toBe(true);
  });

  it('writes the indeterminate state, which has no HTML attribute', () => {
    // React cannot set this declaratively — it is a DOM property only, so a
    // missing effect would leave a tri-state control silently stuck on
    // unchecked with no type error and no failing render.
    const { rerender } = render(<Checkbox aria-label="Select all" indeterminate />);
    const input = screen.getByLabelText('Select all') as HTMLInputElement;
    expect(input.indeterminate).toBe(true);

    rerender(<Checkbox aria-label="Select all" indeterminate={false} />);
    expect(input.indeterminate).toBe(false);
  });

  it('forwards a ref to the input, not to the wrapper', () => {
    let node: HTMLInputElement | null = null;
    render(
      <Checkbox
        aria-label="Ref target"
        ref={(el) => {
          node = el;
        }}
      />
    );
    expect((node as HTMLInputElement | null)?.tagName).toBe('INPUT');
  });
});
