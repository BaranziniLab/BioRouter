import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import ExtensionConfigFields from './ExtensionConfigFields';

/// A shell command is CODE, and this dialog opens directly over the extension
/// row that prints the same string in `font-mono`
/// (settings/extensions/subcomponents/ExtensionItem.tsx). The command was
/// monospace to read and proportional to edit — one value, two faces, and the
/// row is visible behind the modal while you type into it.
///
/// jsdom never runs Tailwind, so asserting a computed font would pass whatever
/// the class says. These assert the CLASS on the control.
describe('ExtensionConfigFields — a command is code, an endpoint is not', () => {
  const props = {
    full_cmd: 'npx -y @modelcontextprotocol/server-filesystem /tmp',
    endpoint: 'https://example.com/sse',
    onChange: vi.fn(),
  };

  it('sets the command field in monospace', () => {
    render(<ExtensionConfigFields type="stdio" {...props} />);
    expect(screen.getByDisplayValue(props.full_cmd).className).toMatch(/font-mono/);
  });

  /// Pinned deliberately. A URL is a different value class from a command and
  /// is body font everywhere else in the app (LocalModelInventory's "Official
  /// URL", the tunnel panel). Without this, "make the command mono" would read
  /// as "make this file mono" and the endpoint would follow it next time.
  it('leaves the endpoint field in the body font', () => {
    render(<ExtensionConfigFields type="sse" {...props} />);
    expect(screen.getByDisplayValue(props.endpoint).className).not.toMatch(/font-mono/);
  });
});
